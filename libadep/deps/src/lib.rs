pub mod proto {
    tonic::include_proto!("adep.depsd.v1");
}

mod artifacts;
mod capsule;
pub mod client;
pub mod defaults;
pub mod error;
pub mod pnpm;
pub mod python;
pub mod server;
pub mod service;
mod util;
