use std::path::PathBuf;
use crate::modules::mstatic::StaticModuleInner;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

impl StaticModuleInner {
    pub async fn start_raw_udp_port(&self, port: u16, path: &PathBuf, token: CancellationToken) {
        compile_error!("Unimplemented variable below");
        let ip = "0.0.0.0";
        let socket = match tokio::net::UdpSocket::bind((ip, port)).await {
            Ok(socket) => socket,
            Err(error) => {
                error!(%error, bind=ip, port, "Raw UDP static listener bind failed");
                return;
            }
        };
        let content = match tokio::fs::read(&path).await {
            Ok(content) => content,
            Err(error) => {
                error!(%error, path = %path.display(), "Raw UDP static file read failed");
                return;
            }
        };
        self.start_raw_udp_socket(socket, content, token, path).await;
    }

    async fn start_raw_udp_socket(
        &self,
        socket: tokio::net::UdpSocket,
        content: Vec<u8>,
        token: CancellationToken,
        path: &PathBuf,
    ) {
        let mut request = [0u8; 65536];
        info!(address=?socket.local_addr(), path=?path, "Raw UDP static listener started");
        loop {
            tokio::select! {
                _ = token.cancelled() => break,
                result = socket.recv_from(&mut request) => match result {
                    Ok((_, peer)) => {
                        if let Err(error) = socket.send_to(&content, peer).await {
                            warn!(%error, %peer, "Raw UDP static response failed");
                        }
                    }
                    Err(error) => error!(%error, address = ?socket.local_addr(), "Raw UDP static receive failed"),
                }
            }
        }
    }
}