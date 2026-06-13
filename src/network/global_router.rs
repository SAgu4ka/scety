use crate::{config::get_services_config::ClientConfig, network::listeners::start_listen_port};
use std::collections::HashSet;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

pub fn start_listen(
    configs: Vec<ClientConfig>,
    expose_version: bool,
    token: CancellationToken,
) -> JoinSet<()> {
    let mut set = JoinSet::new();

    let unique_ports: HashSet<u16> = configs
        .iter()
        .filter_map(|config| config.listen_port)
        .collect();
    let port_to_log: Vec<String> = unique_ports.iter().map(|port| port.to_string()).collect();
    info!(unique_ports=%port_to_log.join(", "), "Start listening on target ports");
    for port in unique_ports {
        let all_config_for_this_port: Vec<ClientConfig> = configs
            .iter()
            .filter(|config| config.listen_port == Some(port))
            .cloned()
            .collect();
        debug!(port=%port, "Starting port listening");
        let token = token.clone();
        set.spawn(async move {
            start_listen_port(port, all_config_for_this_port, expose_version, token).await;
        });
    }
    set
}
