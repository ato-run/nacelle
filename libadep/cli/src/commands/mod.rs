use clap::Subcommand;

pub mod build;
pub mod compose;
pub mod deps;
pub mod dev_pin;
pub mod doctor;
pub mod init;
pub mod keygen;
pub mod manifest;
pub mod capsule;
pub mod pack;
pub mod run;
pub mod sign;
pub mod verify;

#[derive(Subcommand)]
pub enum Command {
    /// Create a new ADEP package skeleton
    Init(init::InitArgs),
    /// Manage Packager v0 Capsules (Create, Plan, Publish)
    Capsule(capsule::CapsuleArgs),
    /// Generate an Ed25519 developer key pair
    Keygen(keygen::KeygenArgs),
    /// Populate manifest files section from dist/
    Build(build::BuildArgs),
    /// Sign the package with the developer key
    Sign(sign::SignArgs),
    /// Verify package integrity and signatures
    Verify(verify::VerifyArgs),
    /// Produce a .adep archive from the working tree
    Pack(pack::PackArgs),
    /// Run the ADEP package with local HTTP server
    Run(run::RunArgs),
    /// Check system requirements for ADEP runtime
    Doctor(doctor::DoctorArgs),
    /// Manage multiple ADEP services (dev mode only)
    Compose(compose::ComposeArgs),
    /// Manage Dev/CI resolver pin records
    DevPin(dev_pin::DevPinArgs),
    /// Manage manifest files
    Manifest(manifest::ManifestArgs),
    /// Inspect or verify dependency artifacts
    Deps(deps::DepsArgs),
}
