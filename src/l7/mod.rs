#![allow(dead_code)]
pub mod config;
pub mod error_pages;
pub mod generate_http_response;
pub mod network;

pub struct L7Module;

impl crate::core::runtime::ProxyModule for L7Module {
    const PROXY_TYPE: &'static str = "L7";
}

pub async fn start(
    configs: Vec<crate::config::get_services_config::ClientConfig>,
    expose_version: bool,
    token: tokio_util::sync::CancellationToken,
) {
    if configs.is_empty() {
        if let Err(error) = network::fallback_server::start_fallback_server(token).await {
            tracing::error!(%error, "Fallback server stopped unexpectedly");
        }
        return;
    }
    let listeners = network::global_router::start_listen(configs, expose_version, token);
    listeners.join_all().await;
}
