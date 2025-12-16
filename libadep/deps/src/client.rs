use crate::proto::depsd_client::DepsdClient;
use crate::proto::{
    ExpandCapsuleRequest, ExpandCapsuleResponse, HealthCheckRequest, HealthCheckResponse,
    InstallPnpmRequest, InstallPnpmResponse, InstallPythonRequest, InstallPythonResponse,
};
use anyhow::Result;
use std::path::PathBuf;
use tokio::time::Duration;
use tonic::transport::{Channel, Endpoint};
use tonic::Status;
use tower::service_fn;

#[cfg(unix)]
use tokio::net::UnixStream;

pub struct Client {
    inner: DepsdClient<Channel>,
}

impl Client {
    pub async fn connect(endpoint: impl AsRef<str>) -> Result<Self> {
        let endpoint = endpoint.as_ref();
        if endpoint.starts_with("unix://") {
            Self::connect_unix(endpoint.trim_start_matches("unix://")).await
        } else if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
            Self::connect_http(endpoint).await
        } else if endpoint.starts_with("tcp://") {
            Self::connect_http(endpoint.trim_start_matches("tcp://")).await
        } else {
            // Allow host:port without scheme.
            let addr = format!("http://{}", endpoint);
            Self::connect_http(&addr).await
        }
    }

    async fn connect_http(endpoint: &str) -> Result<Self> {
        let target = if endpoint.contains("://") {
            endpoint.to_string()
        } else {
            format!("http://{}", endpoint)
        };
        let channel = Endpoint::new(target)?
            .timeout(Duration::from_secs(5))
            .connect()
            .await?;
        Ok(Self {
            inner: DepsdClient::new(channel),
        })
    }

    #[cfg(unix)]
    async fn connect_unix(path: &str) -> Result<Self> {
        let socket_path = PathBuf::from(path);
        let channel = Endpoint::from_static("http://[::]:50051")
            .timeout(Duration::from_secs(5))
            .connect_with_connector(service_fn(move |_| {
                let path = socket_path.clone();
                async move {
                    UnixStream::connect(path).await.map_err(|err| {
                        std::io::Error::other(format!("{err}"))
                    })
                }
            }))
            .await?;
        Ok(Self {
            inner: DepsdClient::new(channel),
        })
    }

    #[cfg(not(unix))]
    async fn connect_unix(_path: &str) -> Result<Self> {
        Err(anyhow::anyhow!(
            "unix domain sockets are not supported on this platform"
        ))
    }

    pub async fn health_check(&mut self) -> Result<HealthCheckResponse, Status> {
        let response = self.inner.health_check(HealthCheckRequest {}).await?;
        Ok(response.into_inner())
    }

    pub async fn expand_capsule(
        &mut self,
        request: ExpandCapsuleRequest,
    ) -> Result<ExpandCapsuleResponse, Status> {
        let response = self.inner.expand_capsule(request).await?;
        Ok(response.into_inner())
    }

    pub async fn install_python(
        &mut self,
        request: InstallPythonRequest,
    ) -> Result<InstallPythonResponse, Status> {
        let response = self.inner.install_python(request).await?;
        Ok(response.into_inner())
    }

    pub async fn install_pnpm(
        &mut self,
        request: InstallPnpmRequest,
    ) -> Result<InstallPnpmResponse, Status> {
        let response = self.inner.install_pnpm(request).await?;
        Ok(response.into_inner())
    }
}
