use std::path::{Component, Path};

// SECURITY: Allowlist for host paths to prevent arbitrary file system access
// This list is now passed from configuration.

/// Validate a path for security
///
/// Checks:
/// 1. Path must be absolute
/// 2. Path must be within allowed_paths
/// 3. Path must not contain traversal components (..)
pub fn validate_path(path_str: &str, allowed_paths: &[String]) -> Result<(), String> {
    let path = Path::new(path_str);

    // 1. Must be absolute
    if !path.is_absolute() {
        return Err(format!("Path must be absolute: {}", path_str));
    }

    // 2. Check for path traversal
    for component in path.components() {
        if let Component::ParentDir = component {
            return Err(format!("Path traversal detected: {}", path_str));
        }
    }

    // 3. Must be in allowlist
    let mut allowed = false;
    for allowed_path in allowed_paths {
        if path.starts_with(allowed_path) {
            allowed = true;
            break;
        }
    }

    if !allowed {
        return Err(format!(
            "Path '{}' is not in the allowed paths: {:?}",
            path_str, allowed_paths
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_path_security() {
        let allowed_paths = vec![
            "/opt/models".to_string(),
            "/mnt/cache".to_string(),
            "/tmp".to_string(),
        ];

        // 1. Test Path Traversal
        let err = validate_path("/opt/models/../etc/passwd", &allowed_paths).unwrap_err();
        assert!(err.contains("Path traversal detected"));

        // 2. Test Allowlist Violation
        let err = validate_path("/etc/shadow", &allowed_paths).unwrap_err();
        assert!(err.contains("not in the allowed paths"));

        // 3. Test Relative Path
        let err = validate_path("relative/path", &allowed_paths).unwrap_err();
        assert!(err.contains("must be absolute"));

        // 4. Test Valid Path
        assert!(validate_path("/opt/models/llama-3.gguf", &allowed_paths).is_ok());
        assert!(validate_path("/mnt/cache/output", &allowed_paths).is_ok());
    }
}
