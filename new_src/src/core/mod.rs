use async_trait::async_trait;
use std::error::Error;
use std::{
    collections::{HashMap, HashSet},
    io::ErrorKind,
    path::PathBuf,
};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

pub mod scety_configs;

/// The outcome of a module's configuration validation check.
pub struct ConfigValidationResult {
    /// Indicates whether the configuration is valid and the module is ready to start.
    pub success: bool,

    /// A list of TCP/UDP ports that this module intends to bind or listen to.
    /// Used by the Scety core to detect port collisions across different modules before launch.
    pub ports: Vec<u16>,

    /// A list of configuration file paths that contained syntax or validation errors (if any).
    pub problem_files: Option<Vec<PathBuf>>,
}

/// The core interface required for all Scety proxy modules.
///
/// Every built-in and custom extension module must implement this trait.
/// Requires `Send + Sync + 'static` to ensure thread safety across Tokio runtime threads.
#[async_trait]
pub trait Module: Send + Sync + 'static {
    /// Returns the unique name of the configuration type or section handled by this module.
    fn config_type_name(&self) -> &str;

    /// Validates the provided configuration slices prior to application startup or reload.
    fn check_config(&self, configs: &[PathBuf]) -> ConfigValidationResult;

    /// Asynchronously runs and executes the module's primary lifecycle.
    ///
    /// # Graceful Shutdown
    /// The module **must** monitor the provided `token`. When `token.is_cancelled()` evaluates to `true`,
    /// the module should stop accepting new connections, finish pending tasks, and exit cleanly.
    ///
    /// # Runtime Reloading
    /// The module **must** also monitor `reload_rx`. When new configuration paths arrive through
    /// this channel, the module should re-validate them and apply changes on the fly.
    ///
    /// # Errors
    /// Returns an `Err` if the module fails to bind sockets, load resources, or start internal services.
    async fn run(
        &mut self,
        token: CancellationToken,
        configs: Vec<PathBuf>,
        reload_rx: mpsc::Receiver<Vec<PathBuf>>,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
}

pub struct ScetyCore {
    modules: HashMap<String, Box<dyn Module>>,
    ports: Option<HashSet<u16>>,
    all_configs: HashMap<String, Vec<PathBuf>>,
    yet_init: bool,
    yet_running: bool,
    cancel_token: CancellationToken,
    reload_senders: HashMap<String, mpsc::Sender<Vec<PathBuf>>>,
}

impl ScetyCore {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
            ports: None,
            all_configs: HashMap::new(),
            yet_init: false,
            yet_running: false,
            cancel_token: CancellationToken::new(),
            reload_senders: HashMap::new(),
        }
    }

    pub fn init(
        &mut self,
        modules: Vec<Box<dyn Module>>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if self.yet_init || self.yet_running {
            return Err(Box::new(std::io::Error::new(
                ErrorKind::ConnectionAborted,
                "Core init yet!",
            )));
        }
        self.add_modules(modules);
        self.check_all_configs()?;
        self.yet_init = true;
        Ok(())
    }

    pub async fn run(
        &mut self,
    ) -> Result<JoinSet<Result<(), Box<dyn Error + Send + Sync>>>, Box<dyn Error + Send + Sync>>
    {
        if !self.yet_init {
            return Err(Box::new(std::io::Error::new(
                ErrorKind::ConnectionAborted,
                "Core not init yet!",
            )));
        }

        let modules_count = self.modules.len();
        let mut set: JoinSet<Result<(), Box<dyn Error + Send + Sync>>> = JoinSet::new();

        for (name, mut module) in self.modules.drain() {
            let token_clone = self.cancel_token.clone();
            let configs_for_module = self.all_configs.get(&name).cloned().unwrap_or_default();

            let (tx, rx) = mpsc::channel(10);
            self.reload_senders.insert(name.clone(), tx);

            set.spawn(async move { module.run(token_clone, configs_for_module, rx).await });
            info!(module = %name, "Module started successfully.");
        }
        self.yet_running = true;
        info!(
            modules_count = modules_count,
            "All modules started successfully. Scety core is running."
        );
        info!(
            ports = ?self.ports,
            "Ports in use by modules."
        );
        Ok(set)
    }

    pub async fn reload(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!("Reloading all module configurations...");

        let new_all_configs = scety_configs::get_all_configs().ok().unwrap_or_default();

        for (name, tx) in self.reload_senders.iter() {
            if let Some(new_configs) = new_all_configs.get(name) {
                if let Err(e) = tx.send(new_configs.clone()).await {
                    error!(module = %name, error = %e, "Failed to send reload event to module");
                }
            }
        }

        self.all_configs = new_all_configs;
        Ok(())
    }

    pub fn shutdown(&self) {
        info!("Initiating graceful shutdown...");
        self.cancel_token.cancel();
    }

    fn check_all_configs(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let all_configs = scety_configs::get_all_configs()
            .ok()
            .unwrap_or(HashMap::new());

        let empty_vec = Vec::new();

        for (name, module) in self.modules.iter() {
            let configs = all_configs.get(name).unwrap_or(&empty_vec);
            let result = module.check_config(configs);

            if !result.success {
                if let Some(problem_files) = result.problem_files {
                    for file in problem_files {
                        error!(module=module.config_type_name(), file=?file, "Configuration validation error in file");
                    }
                }
                error!(
                    module = module.config_type_name(),
                    "Module failed configuration validation."
                );
                return Err(format!(
                    "Module '{}' failed configuration validation.",
                    module.config_type_name()
                )
                .into());
            }

            for port in result.ports {
                if !self.ports.get_or_insert_with(HashSet::new).insert(port) {
                    error!(
                        module = module.config_type_name(),
                        port = port,
                        "Port collision detected: Port is already in use by another module."
                    );
                    return Err(format!(
                        "Port collision detected: Port {} is already in use by another module.",
                        port
                    )
                    .into());
                }
            }
        }
        self.all_configs = all_configs;
        Ok(())
    }

    fn add_modules(&mut self, modules: Vec<Box<dyn Module>>) {
        for module in modules {
            let name = module.config_type_name().to_string();
            self.modules.insert(name, module);
        }
    }
}
