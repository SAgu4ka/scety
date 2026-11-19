use crate::{
    core::host_router::HostRouter,
    modules::mstatic::{
        StaticModuleInner,
        config::{Files, HttpVersion, SslConfig},
    },
};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

impl StaticModuleInner {
    pub async fn start_tcp_port(
        &self,
        port: u16,
        ssl: &Option<SslConfig>,
        files: &Files,
        http_version: &HttpVersion,
        token: CancellationToken,
    ) {
        if ssl.is_some() {
            self.start_tcp_listener_with_tls(port, ssl, files, http_version, token);
        } else {
            self.start_tcp_listener_without_tls(port, files, http_version, token);
        }
    }

    async fn start_tcp_listener_with_tls(
        &self,
        port: u16,
        ssl: &Option<SslConfig>,
        files: &Files,
        http_version: &HttpVersion,
        token: CancellationToken,
    ) {
        todo!()
    }

    async fn start_tcp_listener_without_tls(
        &self,
        port: u16,
        files: &Files,
        http_version: &HttpVersion,
        token: CancellationToken,
    ) {
        // compile_error!("Unimplemented variable below");
        let ip = "0.0.0.0";
        let listener = match TcpListener::bind((ip, port)).await {
            Ok(listener) => listener,
            Err(error) => {
                error!(%error, port, "Static listener bind failed");
                return;
            }
        };
        todo!();
        //let mut router = HostRouter::new();
        //let router = Arc::new(router);
        info!(address = ?listener.local_addr(), "Static listener started");
    }
}
