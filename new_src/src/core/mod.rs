use crate::DynResult;
use async_trait::async_trait;
use std::time::Duration;
use std::{
    collections::{HashMap, HashSet},
    io::ErrorKind,
    path::PathBuf,
};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

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
    ) -> DynResult<()>;
}

pub struct ScetyCore {
    modules: HashMap<String, Box<dyn Module>>,
    ports: HashSet<u16>,
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
            ports: HashSet::new(),
            all_configs: HashMap::new(),
            yet_init: false,
            yet_running: false,
            cancel_token: CancellationToken::new(),
            reload_senders: HashMap::new(),
        }
    }

    pub fn init(&mut self, modules: Vec<Box<dyn Module>>) -> DynResult<()> {
        if self.yet_init || self.yet_running {
            return Err(std::io::Error::new(
                ErrorKind::ConnectionAborted,
                "Core already initialized or running",
            )
            .into());
        }

        let mut new_modules: HashMap<String, Box<dyn Module>> = HashMap::new();
        for module in modules {
            let name = module.config_type_name().to_string();
            if new_modules.contains_key(&name) {
                return Err(format!("Duplicate module name: {}", name).into());
            }
            new_modules.insert(name, module);
        }

        let all_configs = scety_configs::get_all_configs()?;
        let mut new_ports = HashSet::new();

        for (name, module) in new_modules.iter() {
            let empty = Vec::new();
            let configs = all_configs.get(name).unwrap_or(&empty);
            let result = module.check_config(configs);

            if !result.success {
                if let Some(problem_files) = result.problem_files {
                    for file in problem_files {
                        error!(module = module.config_type_name(), file = ?file,
                            "Configuration validation error in file");
                    }
                }
                return Err(format!(
                    "Module '{}' failed configuration validation.",
                    module.config_type_name()
                )
                .into());
            }

            for port in result.ports {
                if !new_ports.insert(port) {
                    return Err(format!(
                        "Port collision detected: Port {} is already in use.",
                        port
                    )
                    .into());
                }
            }
        }

        self.modules = new_modules;
        self.all_configs = all_configs;
        self.ports = new_ports;
        self.yet_init = true;
        Ok(())
    }

    pub async fn run(&mut self) -> DynResult<JoinSet<DynResult<()>>> {
        if !self.yet_init {
            return Err(Box::new(std::io::Error::new(
                ErrorKind::ConnectionAborted,
                "Core not init yet!",
            )));
        } else if self.yet_running {
            return Err(Box::new(std::io::Error::new(
                ErrorKind::ConnectionAborted,
                "Core running yet!",
            )));
        }

        let modules_count = self.modules.len();
        if modules_count == 0 {
            warn!("No modules configured; core will run idle");
        }
        let mut set: JoinSet<DynResult<()>> = JoinSet::new();

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

    pub async fn reload(&mut self) -> DynResult<()> {
        info!("Reloading all module configurations...");

        let new_all_configs = scety_configs::get_all_configs()?;

        for (name, tx) in self.reload_senders.iter() {
            let new_configs = new_all_configs.get(name).cloned().unwrap_or_default();
            match tokio::time::timeout(Duration::from_secs(2), tx.send(new_configs)).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => error!(module = %name, error = %e, "Reload send failed"),
                Err(_) => error!(module = %name, "Reload send timed out; module stuck?"),
            }
        }
        info!("All module configurations reloaded successfully");

        self.all_configs = new_all_configs;
        Ok(())
    }

    pub fn shutdown(&self) {
        info!("Initiating graceful shutdown...");
        self.cancel_token.cancel();
    }
}
