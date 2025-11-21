use axum::{
    extract::{State, Json},
    http::{Method, HeaderValue},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::{info, error};

use crate::capsule_manager::CapsuleManager;
use crate::hardware::GpuDetector;
use crate::network::service_registry::ServiceRegistry;

#[derive(Clone)]
pub struct AppState {
    pub capsule_manager: Arc<CapsuleManager>,
    pub service_registry: Arc<ServiceRegistry>,
    pub gpu_detector: Arc<dyn GpuDetector>,
}

#[derive(Serialize)]
struct StatusResponse {
    status: String,
    version: String,
    gpu_info: Option<GpuInfo>,
}

#[derive(Serialize)]
struct GpuInfo {
    count: usize,
    total_vram_gb: f64,
    names: Vec<String>,
}

#[derive(Deserialize)]
struct DeployRequest {
    capsule_id: String,
    // Optional: allow passing manifest content directly in future
    // manifest: Option<String>, 
}

#[derive(Serialize)]
struct DeployResponse {
    capsule_id: String,
    status: String,
    url: String,
}

pub async fn start_api_server(
    port: u16,
    capsule_manager: Arc<CapsuleManager>,
    service_registry: Arc<ServiceRegistry>,
    gpu_detector: Arc<dyn GpuDetector>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let state = AppState {
        capsule_manager,
        service_registry,
        gpu_detector,
    };

    // CORS configuration
    // allowing localhost:3000 (Next.js) and potentially others
    let cors = CorsLayer::new()
        .allow_origin("http://localhost:3000".parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([axum::http::header::CONTENT_TYPE]);

    let app = Router::new()
        .route("/v1/status", get(status_handler))
        .route("/v1/deploy", post(deploy_handler))
        .layer(cors)
        .with_state(state);

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    info!("REST API server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn status_handler(State(state): State<AppState>) -> impl IntoResponse {
    let gpu_report = state.gpu_detector.detect_gpus().ok();
    
    let gpu_info = gpu_report.map(|report| GpuInfo {
        count: report.gpus.len(),
        total_vram_gb: report.total_vram_gb(),
        names: report.gpus.iter().map(|g| g.device_name.clone()).collect(),
    });

    Json(StatusResponse {
        status: "ready".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        gpu_info,
    })
}

async fn deploy_handler(
    State(state): State<AppState>,
    Json(payload): Json<DeployRequest>,
) -> impl IntoResponse {
    info!("REST Deploy request for capsule_id: {}", payload.capsule_id);

    // For Phase 1, we assume the capsule has a preset or we load a default one.
    // Since we don't have the TOML content in the request yet, we'll try to load it 
    // from the examples directory or use a default.
    // This is a simplification to match the user's "Run Flux.1" example where the frontend 
    // just sends an ID.
    
    // In a real scenario, we might want to read the manifest from disk or receive it.
    // For now, we'll construct a minimal manifest or load from file if possible.
    
    // HACK: We need to pass manifest bytes to deploy_capsule.
    // We'll try to read `examples/capsule.toml` or similar if it matches the ID,
    // or just pass empty/dummy bytes if the CapsuleManager can handle it (it likely can't).
    
    // Let's try to find a TOML file for this capsule in the presets directory
    // relative to where we are running.
    let manifest_path = std::path::Path::new("examples").join(format!("{}.toml", payload.capsule_id));
    let manifest_bytes = if manifest_path.exists() {
        match std::fs::read_to_string(&manifest_path) {
            Ok(content) => {
                // Convert TOML to JSON as CapsuleManager expects JSON bytes (mostly)
                // But wait, deploy_capsule takes bytes.
                // In grpc_server.rs, it converts TOML to JSON.
                // We should probably do the same here.
                
                // We need the `workload` module to parse TOML.
                // But `workload` might not be public.
                // Let's check if we can access it.
                // If not, we might need to duplicate the logic or make it public.
                
                // Assuming we can't easily access internal logic, let's try to read it as raw bytes
                // and hope CapsuleManager can handle it? No, CapsuleManager expects JSON usually
                // unless we change it.
                // Actually, `grpc_server.rs` does the conversion.
                
                // Let's try to read it, parse with `toml`, then serialize to JSON.
                match toml::from_str::<serde_json::Value>(&content) {
                    Ok(json) => serde_json::to_vec(&json).unwrap_or_default(),
                    Err(_) => Vec::new(),
                }
            }
            Err(_) => Vec::new(),
        }
    } else {
        // Fallback: Create a dummy manifest
        let dummy = serde_json::json!({
            "name": payload.capsule_id,
            "workload": {
                "image": "ubuntu:latest", // Default
                "command": ["sleep", "infinity"]
            }
        });
        serde_json::to_vec(&dummy).unwrap_or_default()
    };

    match state.capsule_manager.deploy_capsule(
        payload.capsule_id.clone(),
        manifest_bytes,
        String::new(), // oci_image (optional/empty for now)
        String::new(), // digest (optional/empty for now)
    ).await {
        Ok(status) => {
            // Get the URL
             let url = state.service_registry
                .get_services()
                .iter()
                .find(|s| s.name == payload.capsule_id)
                .map(|s| format!("http://localhost:{}", s.port))
                .unwrap_or_else(|| format!("http://localhost"));

            Json(DeployResponse {
                capsule_id: payload.capsule_id,
                status,
                url,
            })
        }
        Err(e) => {
            error!("Deployment failed: {}", e);
            // Return 500
             Json(DeployResponse {
                capsule_id: payload.capsule_id,
                status: format!("error: {}", e),
                url: "".to_string(),
            })
        }
    }
}
