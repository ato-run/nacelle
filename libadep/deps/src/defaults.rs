use libadep_core::deps::{DependencyDefaults, ResolveReport, ResolveWarning};
use libadep_core::error::AdepError;
use libadep_core::manifest::{DefaultOptions, Manifest};

/// Blocking resolver that performs minimal validation on dep_capsules.
#[derive(Debug, Default)]
pub struct StaticDefaults;

impl StaticDefaults {
    pub fn new() -> Self {
        Self
    }
}

impl DependencyDefaults for StaticDefaults {
    fn apply_defaults(
        &self,
        manifest: &mut Manifest,
        _opts: &DefaultOptions<'_>,
    ) -> Result<ResolveReport, AdepError> {
        let mut warnings = Vec::new();
        let mut mutated = false;

        if manifest.dep_capsules.is_empty() {
            manifest
                .dep_capsules
                .push("adep://autofill/default:latest".into());
            warnings.push(ResolveWarning {
                code: "DEPS-AUTOFILL-CAPSULE".into(),
                message: "added default dependency capsule reference".into(),
            });
            mutated = true;
            return Ok(ResolveReport::new(warnings, mutated));
        }

        for (index, reference) in manifest.dep_capsules.iter_mut().enumerate() {
            let valid = reference.starts_with("oci://") || reference.starts_with("adep://");
            if !valid {
                *reference = format!("adep://autofill/{index}");
                warnings.push(ResolveWarning {
                    code: "DEPS-AUTOFILL-CAPSULE".into(),
                    message: format!(
                        "dep_capsules[{index}] invalid URI; replaced with adep://autofill/{index}"
                    ),
                });
                mutated = true;
            }
        }

        Ok(ResolveReport::new(warnings, mutated))
    }
}
