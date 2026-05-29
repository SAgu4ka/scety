use crate::{config::get_servises_config::ClientConfig, network::listeners::start_listen_port};
use std::collections::HashSet;

pub fn start_listen(configs: Vec<ClientConfig>, expose_version: bool) {
    let unique_ports: HashSet<u16> = configs
        .iter()
        .map(|config| config.listen_port)
        .collect();

    for port in unique_ports {
        let all_config_for_this_port: Vec<ClientConfig> = configs
            .iter()
            .filter(|config| config.listen_port == port)
            .cloned()
            .collect();

        start_listen_port(port, all_config_for_this_port, expose_version);
    }
}