use crate::proto::depsd_server::DepsdServer;
use crate::service::DepsdService;
use anyhow::{Context, Result};
use std::net::SocketAddr;
use tonic::transport::Server;

#[derive(Debug, Clone)]
pub enum TransportAddress {
    Tcp(SocketAddr),
    #[cfg(unix)]
    Unix(std::path::PathBuf),
}

/// gRPC サーバを指定アドレスで起動する。
pub async fn serve(transport: TransportAddress, service: DepsdService) -> Result<()> {
    match transport {
        TransportAddress::Tcp(addr) => {
            Server::builder()
                .add_service(DepsdServer::new(service))
                .serve(addr)
                .await?;
            Ok(())
        }
        #[cfg(unix)]
        TransportAddress::Unix(path) => {
            use std::fs;
            use tokio::net::UnixListener;
            use tokio_stream::wrappers::UnixListenerStream;

            if path.exists() {
                fs::remove_file(&path)
                    .with_context(|| format!("failed to remove stale socket {}", path.display()))?;
            }
            let listener = UnixListener::bind(&path).map_err(|err| {
                anyhow::anyhow!("failed to bind unix socket {}: {err}", path.display())
            })?;
            let incoming = UnixListenerStream::new(listener);
            Server::builder()
                .add_service(DepsdServer::new(service))
                .serve_with_incoming(incoming)
                .await?;
            Ok(())
        }
        #[cfg(not(unix))]
        TransportAddress::Unix(_path) => Err(anyhow::anyhow!(
            "unix domain sockets are not supported on this platform"
        )),
    }
}
