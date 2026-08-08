use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::time::Duration;
use toml::Value;
use tracing::{error, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L4ServiceConfig {
    pub name: String,
    pub bind: SocketAddr,
    pub protocol: L4Protocol,
    pub reuse_port: bool,
    pub tcp_nodelay: bool,
    pub zero_copy: bool,
    pub upstreams: Vec<SocketAddr>,
    pub lb_strategy: LbStrategy,
    pub send_proxy_protocol: Option<ProxyProtocolVersion>,
    pub connect_timeout: Duration,
    pub idle_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum L4Protocol {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LbStrategy {
    RoundRobin,
    LeastConnections,
    IpHash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProxyProtocolVersion {
    V1,
    V2,
}

pub fn validate_l4_configs(configs: &[Value]) -> bool {
    let mut has_errors = false;
    let mut l4_count = 0;
    let mut seen_names = HashSet::new();
    let mut seen_binds = HashSet::new();

    for (idx, cnfg) in configs.iter().enumerate() {
        let table = match cnfg.as_table() {
            Some(t) => t,
            None => {
                error!(
                    config_idx = idx,
                    "Configuration root must be a valid TOML table"
                );
                has_errors = true;
                continue;
            }
        };

        let is_l4 = table
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("proxy_type"))
            .and_then(|(_, v)| v.as_str())
            .map(|s| s.eq_ignore_ascii_case("L4"))
            .unwrap_or(false);

        if !is_l4 {
            continue;
        }

        l4_count += 1;

        let name = match get_str_case_insensitive(table, "name") {
            Some(n) if !n.trim().is_empty() => n.trim(),
            _ => {
                error!(config_idx = idx, "Missing or empty required field 'name'");
                has_errors = true;
                continue;
            }
        };

        if !seen_names.insert(name.to_string()) {
            error!(service = %name, "Duplicate service name found");
            has_errors = true;
        }

        let bind_str = match get_str_case_insensitive(table, "bind") {
            Some(b) => b,
            None => {
                error!(service = %name, "Missing required field 'bind'");
                has_errors = true;
                continue;
            }
        };

        let bind_addr: SocketAddr = match bind_str.parse() {
            Ok(addr) => addr,
            Err(e) => {
                error!(service = %name, bind = %bind_str, error = %e, "Invalid 'bind' address format (expected IP:Port)");
                has_errors = true;
                continue;
            }
        };

        if !seen_binds.insert(bind_addr) {
            error!(service = %name, bind = %bind_addr, "Address 'bind' is already assigned to another L4 service");
            has_errors = true;
        }

        let upstreams_val = match get_field_case_insensitive(table, "upstreams") {
            Some(v) => v,
            None => {
                error!(service = %name, "Missing required field 'upstreams'");
                has_errors = true;
                continue;
            }
        };

        let mut valid_upstreams_count = 0;
        if let Some(arr) = upstreams_val.as_array() {
            for (u_idx, item) in arr.iter().enumerate() {
                if let Some(s) = item.as_str() {
                    if s.parse::<SocketAddr>().is_ok() {
                        valid_upstreams_count += 1;
                    } else {
                        error!(service = %name, upstream = %s, upstream_idx = u_idx, "Invalid upstream address format (expected IP:Port)");
                        has_errors = true;
                    }
                } else {
                    error!(service = %name, upstream_idx = u_idx, "Upstream entry must be a string");
                    has_errors = true;
                }
            }
        } else {
            error!(service = %name, "Field 'upstreams' must be an array of strings");
            has_errors = true;
        }

        if valid_upstreams_count == 0 {
            error!(service = %name, "Service 'upstreams' array contains no valid SocketAddr entries");
            has_errors = true;
        }
    }

    if !has_errors {
        info!(
            validated_services = l4_count,
            "L4 configuration check passed successfully"
        );
    } else {
        error!("L4 configuration check failed due to validation errors");
    }

    !has_errors
}

pub async fn parse_l4_confs(configs: Vec<Value>) -> Vec<L4ServiceConfig> {
    if !validate_l4_configs(&configs) {
        error!("Aborting L4 configuration parsing due to validation errors");
        return Vec::new();
    }

    let mut ready_cfgs = Vec::new();

    for cnfg in configs {
        let table = match cnfg.as_table() {
            Some(t) => t,
            None => continue,
        };

        let is_l4 = table
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("proxy_type"))
            .and_then(|(_, v)| v.as_str())
            .map(|s| s.eq_ignore_ascii_case("L4"))
            .unwrap_or(false);

        if !is_l4 {
            continue;
        }

        let name = match get_str_case_insensitive(table, "name") {
            Some(n) => n.trim().to_string(),
            None => continue,
        };

        let bind_addr: SocketAddr =
            match get_str_case_insensitive(table, "bind").and_then(|b| b.parse().ok()) {
                Some(addr) => addr,
                None => continue,
            };

        let upstreams_val =
            match get_field_case_insensitive(table, "upstreams").and_then(|v| v.as_array()) {
                Some(arr) => arr,
                None => continue,
            };

        let parsed_upstreams: Vec<SocketAddr> = upstreams_val
            .iter()
            .filter_map(|item| item.as_str())
            .filter_map(|s| s.parse().ok())
            .collect();

        if parsed_upstreams.is_empty() {
            continue;
        }

        let protocol = get_str_case_insensitive(table, "protocol")
            .map(|s| match s.to_lowercase().as_str() {
                "udp" => L4Protocol::Udp,
                _ => L4Protocol::Tcp,
            })
            .unwrap_or(L4Protocol::Tcp);

        let reuse_port = get_bool_case_insensitive(table, "reuse_port").unwrap_or(true);
        let tcp_nodelay = get_bool_case_insensitive(table, "tcp_nodelay").unwrap_or(true);
        let zero_copy = get_bool_case_insensitive(table, "zero_copy").unwrap_or(true);

        let lb_strategy = get_str_case_insensitive(table, "lb_strategy")
            .map(|s| match s.to_lowercase().as_str() {
                "round_robin" | "rr" => LbStrategy::RoundRobin,
                "ip_hash" => LbStrategy::IpHash,
                _ => LbStrategy::LeastConnections,
            })
            .unwrap_or(LbStrategy::LeastConnections);

        let send_proxy_protocol =
            get_str_case_insensitive(table, "send_proxy_protocol").and_then(|s| {
                match s.to_lowercase().as_str() {
                    "v1" => Some(ProxyProtocolVersion::V1),
                    "v2" => Some(ProxyProtocolVersion::V2),
                    _ => None,
                }
            });

        let connect_timeout_secs = get_int_case_insensitive(table, "connect_timeout").unwrap_or(3);
        let idle_timeout_secs = get_int_case_insensitive(table, "idle_timeout").unwrap_or(300);

        ready_cfgs.push(L4ServiceConfig {
            name,
            bind: bind_addr,
            protocol,
            reuse_port,
            tcp_nodelay,
            zero_copy,
            upstreams: parsed_upstreams,
            lb_strategy,
            send_proxy_protocol,
            connect_timeout: Duration::from_secs(connect_timeout_secs as u64),
            idle_timeout: Duration::from_secs(idle_timeout_secs as u64),
        });
    }

    ready_cfgs
}

fn get_field_case_insensitive<'a>(
    table: &'a toml::map::Map<String, Value>,
    key: &str,
) -> Option<&'a Value> {
    table.iter().find_map(|(k, v)| {
        if k.eq_ignore_ascii_case(key) {
            Some(v)
        } else {
            None
        }
    })
}

fn get_str_case_insensitive<'a>(
    table: &'a toml::map::Map<String, Value>,
    key: &str,
) -> Option<&'a str> {
    get_field_case_insensitive(table, key).and_then(|v| v.as_str())
}

fn get_bool_case_insensitive(table: &toml::map::Map<String, Value>, key: &str) -> Option<bool> {
    get_field_case_insensitive(table, key).and_then(|v| v.as_bool())
}

fn get_int_case_insensitive(table: &toml::map::Map<String, Value>, key: &str) -> Option<i64> {
    get_field_case_insensitive(table, key).and_then(|v| v.as_integer())
}
