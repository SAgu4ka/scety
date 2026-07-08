use crate::{
    config::get_services_config::{ClientConfig, SslConfig},
    network::listeners::{SslMode, start_listen_port},
};
use std::collections::HashSet;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

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

        let ssl_mode = resolve_ssl_mode(&all_config_for_this_port, port);

        debug!(port=%port, "Starting port listening");
        let token = token.clone();
        set.spawn(async move {
            start_listen_port(
                port,
                all_config_for_this_port,
                expose_version,
                ssl_mode,
                token,
            )
            .await;
        });
    }
    set
}

fn resolve_ssl_mode(configs: &[ClientConfig], port: u16) -> SslMode {
    let ssl_key = format!("ssl_{}", port);

    let ssl_config = configs
        .iter()
        .find_map(|c| c.ssl.clone().or_else(|| c.ssl_ports.get(&ssl_key).cloned()));

    let Some(ssl) = ssl_config else {
        return SslMode::None;
    };

    if configs.len() > 1
        && configs
            .iter()
            .any(|c| c.ssl.is_none() && !c.ssl_ports.contains_key(&ssl_key))
    {
        warn!(
            port = %port,
            "Some hosts sharing this port don't declare SSL settings; the whole port will be served over TLS anyway"
        );
    }

    resolve_mode(ssl)
}

fn resolve_mode(ssl: SslConfig) -> SslMode {
    if ssl.acme.unwrap_or(false) {
        SslMode::Acme(ssl)
    } else {
        SslMode::Manual(ssl)
    }
}
