/// OCI (Open Container Initiative) runtime specification module
///
/// This module handles:
/// - Generation of OCI config.json files for capsule execution
/// - Docker Registry API v2 client for image pulling
/// - Layer extraction and caching
/// - Image management (pull, cache, rootfs preparation)
pub mod spec_builder;
pub mod registry;
pub mod layer;
pub mod cache;
pub mod image;

pub use spec_builder::build_oci_spec;
pub use registry::{ImageRef, ImageManifest, RegistryClient, RegistryError};
pub use layer::{LayerExtractor, LayerError};
pub use cache::{LayerCache, CacheError};
pub use image::{ImageManager, ImageError, PulledImage};
