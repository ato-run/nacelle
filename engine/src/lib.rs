pub mod adep;
pub mod api_server;
pub mod artifact;
pub mod auth;
pub mod billing;
pub mod capsule_manager;
pub mod cloud;
pub mod config;
// pub mod coordinator_service;  // Disabled: proto definitions not present in capsuled/proto
pub mod downloader; // Enabled for Phase 2
pub mod grpc_server; // Enabled for Phase 2
pub mod hardware;
pub mod logs;
pub mod manifest;
pub mod metrics;
pub mod network;
pub mod oci;
pub mod process_supervisor;
pub mod proto;
pub mod runplan;
pub mod runtime;
pub mod security;
// pub mod status_reporter;
pub mod storage;
pub mod wasm_host;
pub mod workload;
