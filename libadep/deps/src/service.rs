use crate::error::DepsdError;
use crate::pnpm::PnpmHandler;
use crate::proto::depsd_server::Depsd;
use crate::proto::{
    ExpandCapsuleRequest, ExpandCapsuleResponse, HealthCheckRequest, HealthCheckResponse,
    InstallPnpmRequest, InstallPnpmResponse, InstallPythonRequest, InstallPythonResponse,
    OperationError,
};
use crate::python::PythonHandler;
use anyhow::Result;
use libadep_cas::CasError;
use libadep_observability::{AuditEvent, AuditWriter, MetricsRegistry};
use prost::bytes::Bytes;
use prost::Message;
use serde_json::Error as SerdeError;
use std::io::Error as IoError;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tonic::{Code, Request, Response, Status};

/// サービス初期化時の設定。CLI 側からソケットや CAS ルート情報を受け取る。
#[derive(Clone, Debug)]
pub struct ServiceConfig {
    pub cas_root: Option<std::path::PathBuf>,
}

#[derive(Clone)]
pub struct DepsdService {
    config: Arc<RwLock<ServiceConfig>>,
    python: PythonHandler,
    pnpm: PnpmHandler,
    audit: Option<Arc<AuditWriter>>,
    metrics: Option<Arc<MetricsRegistry>>,
}

impl DepsdService {
    pub fn new(config: ServiceConfig) -> Self {
        let audit = match AuditWriter::new(None) {
            Ok(writer) => Some(Arc::new(writer)),
            Err(err) => {
                eprintln!("depsd: failed to initialize audit writer: {err:?}");
                None
            }
        };
        let metrics = match MetricsRegistry::new(None) {
            Ok(registry) => Some(Arc::new(registry)),
            Err(err) => {
                eprintln!("depsd: failed to initialize metrics registry: {err:?}");
                None
            }
        };
        Self {
            config: Arc::new(RwLock::new(config)),
            python: PythonHandler::new(),
            pnpm: PnpmHandler::new(),
            audit,
            metrics,
        }
    }

    pub async fn update_config(&self, update: ServiceConfig) -> Result<()> {
        let mut cfg = self.config.write().await;
        *cfg = update;
        Ok(())
    }

    fn record_install_event(
        &self,
        ecosystem: &str,
        outcome: &str,
        duration: std::time::Duration,
        error_code: Option<String>,
        message: Option<String>,
    ) {
        let error_code_text = error_code;
        let message_text = message;
        if let Some(audit) = &self.audit {
            let duration_ms = duration.as_millis().min(u128::from(u64::MAX)) as u64;
            let event_name = format!("deps.install.{ecosystem}");
            let audit_event = AuditEvent {
                ts: String::new(),
                component: "depsd",
                event: &event_name,
                coords: None,
                outcome: Some(outcome),
                error_code: error_code_text.as_deref(),
                duration_ms: Some(duration_ms),
                bytes: None,
                details: message_text.as_deref(),
            };
            if let Err(err) = audit.write_event(&audit_event) {
                eprintln!("failed to write audit event: {err:?}");
            }
        }
        if let Some(metrics) = &self.metrics {
            let _ = metrics.inc_counter(
                "adep_deps_install_total",
                &[("ecosystem", ecosystem), ("outcome", outcome)],
                1.0,
            );
            let _ = metrics.set_gauge(
                "adep_deps_install_duration_seconds",
                &[("ecosystem", ecosystem)],
                duration.as_secs_f64(),
            );
        }
    }

    fn map_error(err: anyhow::Error) -> (Status, DepsdError) {
        let classified = Self::classify_error(&err);
        let detail = OperationError {
            code: classified.code.to_string(),
            message: classified.message.clone(),
        };
        let data = detail.encode_to_vec();
        let status = if data.is_empty() {
            Status::new(classified.status, classified.message.clone())
        } else {
            Status::with_details(
                classified.status,
                classified.message.clone(),
                Bytes::from(data),
            )
        };
        (status, classified)
    }

    fn classify_error(err: &anyhow::Error) -> DepsdError {
        if let Some(depsd) = err.downcast_ref::<DepsdError>() {
            return depsd.clone();
        }
        for cause in err.chain() {
            if let Some(depsd) = cause.downcast_ref::<DepsdError>() {
                return depsd.clone();
            }
            if let Some(cas_err) = cause.downcast_ref::<CasError>() {
                return DepsdError::from_cas_error(cas_err);
            }
            if let Some(io_err) = cause.downcast_ref::<IoError>() {
                return DepsdError::new("E_ADEP_DEPS_IO", io_err.to_string())
                    .with_status(Code::Internal);
            }
            if let Some(json_err) = cause.downcast_ref::<SerdeError>() {
                return DepsdError::new("E_ADEP_DEPS_INVALID_CAPSULE", json_err.to_string())
                    .with_status(Code::InvalidArgument);
            }
        }
        DepsdError::new("E_ADEP_DEPS_INTERNAL", err.to_string()).with_status(Code::Internal)
    }
}

#[tonic::async_trait]
impl Depsd for DepsdService {
    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse {
            status: "ok".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }))
    }

    async fn expand_capsule(
        &self,
        _request: Request<ExpandCapsuleRequest>,
    ) -> Result<Response<ExpandCapsuleResponse>, Status> {
        Err(Status::unimplemented("ExpandCapsule not implemented yet"))
    }

    async fn install_python(
        &self,
        _request: Request<InstallPythonRequest>,
    ) -> Result<Response<InstallPythonResponse>, Status> {
        let req = _request.into_inner();
        let start = Instant::now();
        match self.python.install(&req) {
            Ok(response) => {
                self.record_install_event("python", "success", start.elapsed(), None, None);
                Ok(Response::new(response))
            }
            Err(err) => {
                let (status, detail) = Self::map_error(err);
                self.record_install_event(
                    "python",
                    "failure",
                    start.elapsed(),
                    Some(detail.code.to_string()),
                    Some(detail.message.clone()),
                );
                Err(status)
            }
        }
    }

    async fn install_pnpm(
        &self,
        _request: Request<InstallPnpmRequest>,
    ) -> Result<Response<InstallPnpmResponse>, Status> {
        let req = _request.into_inner();
        let start = Instant::now();
        match self.pnpm.install(&req) {
            Ok(response) => {
                self.record_install_event("pnpm", "success", start.elapsed(), None, None);
                Ok(Response::new(response))
            }
            Err(err) => {
                let (status, detail) = Self::map_error(err);
                self.record_install_event(
                    "pnpm",
                    "failure",
                    start.elapsed(),
                    Some(detail.code.to_string()),
                    Some(detail.message.clone()),
                );
                Err(status)
            }
        }
    }
}
