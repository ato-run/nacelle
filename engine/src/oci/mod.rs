/// OCI (Open Container Initiative) runtime specification module
///
/// This module handles the generation of OCI config.json files for capsule execution.
/// It translates adep.json compute configuration into OCI runtime specifications,
/// including GPU passthrough via NVIDIA Container Toolkit hooks.
pub mod spec_builder;

pub use spec_builder::build_oci_spec;
