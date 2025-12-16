use serde::Deserialize;
use std::collections::HashMap;
use std::mem;
use std::slice;
use std::str;
use validator::Validate;

// -----------------------------------------------------------------------------
// Memory Management Exports
// -----------------------------------------------------------------------------

/// Allocate memory in Wasm linear memory
///
/// # Arguments
/// * `len` - Number of bytes to allocate
///
/// # Returns
/// * Pointer to the allocated memory
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    mem::forget(buf); // Prevent Rust from freeing this memory
    ptr
}

/// Deallocate memory in Wasm linear memory
///
/// # Arguments
/// * `ptr` - Pointer to the memory to deallocate
/// * `len` - Number of bytes to deallocate
///
/// # Safety
/// * `ptr` must have been allocated by `alloc`.
/// * `len` must be the same size that was passed to `alloc`.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    let _ = Vec::from_raw_parts(ptr, len, len);
}

// -----------------------------------------------------------------------------
// Data Structures (Mirroring Engine/Client definitions)
// -----------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate)]
pub struct AdepManifest {
    #[validate(length(min = 1, message = "Name is required"))]
    pub name: String,

    pub scheduling: Option<SchedulingConfig>,

    #[validate]
    pub compute: ComputeConfig,

    #[serde(default)]
    #[validate]
    pub volumes: Vec<AdepVolume>,

    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct SchedulingConfig {
    #[validate]
    pub gpu: Option<GpuConstraints>,
    pub strategy: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct GpuConstraints {
    #[validate(range(min = 0))]
    pub vram_min_gb: u64,
    pub cuda_version_min: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ComputeConfig {
    #[validate(length(min = 1, message = "Image is required"))]
    pub image: String,

    #[serde(default)]
    pub args: Vec<String>,

    #[serde(default)]
    pub env: Vec<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct AdepVolume {
    #[serde(rename = "type")]
    #[validate(length(min = 1))]
    pub volume_type: String,

    #[validate(length(min = 1))]
    pub source: String,

    #[validate(length(min = 1))]
    pub destination: String,

    pub readonly: bool,
}

// -----------------------------------------------------------------------------
// Validation Logic
// -----------------------------------------------------------------------------

/// Validate an adep.json manifest
///
/// # Arguments
/// * `ptr` - Pointer to the JSON string in Wasm memory
/// * `len` - Length of the JSON string
///
/// # Returns
/// * `1` if valid
/// * `0` if invalid
///
/// # Safety
/// * `ptr` must point to a valid UTF-8 string of length `len` in Wasm memory.
#[no_mangle]
pub unsafe extern "C" fn validate_manifest(ptr: *const u8, len: usize) -> u32 {
    // Safety: We assume the host has written valid UTF-8 bytes to the memory
    // allocated via `alloc`.
    let bytes = unsafe { slice::from_raw_parts(ptr, len) };
    let json_str = match str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return 0, // Invalid UTF-8
    };

    // Parse JSON
    let manifest: AdepManifest = match serde_json::from_str(json_str) {
        Ok(m) => m,
        Err(_) => return 0, // Invalid JSON structure
    };

    // Validate constraints
    match manifest.validate() {
        Ok(_) => 1,
        Err(_) => 0, // Validation failed
    }
}
