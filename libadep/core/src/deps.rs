use crate::error::AdepError;
use crate::manifest::{DefaultOptions, Manifest, ManifestWarning};

/// Warning produced when dependency defaults detect a potential issue.
#[derive(Debug, Clone)]
pub struct ResolveWarning {
    pub code: String,
    pub message: String,
}

/// Result produced by a dependency defaults resolver.
#[derive(Debug, Clone, Default)]
pub struct ResolveReport {
    pub warnings: Vec<ResolveWarning>,
    pub mutated: bool,
}

impl ResolveReport {
    pub fn new(warnings: Vec<ResolveWarning>, mutated: bool) -> Self {
        Self { warnings, mutated }
    }
}

/// Trait for dependency default handlers. Blocking by default.
pub trait DependencyDefaults {
    #[allow(clippy::result_large_err)]
    fn apply_defaults(
        &self,
        manifest: &mut Manifest,
        opts: &DefaultOptions<'_>,
    ) -> Result<ResolveReport, AdepError>;
}

/// No-op stub used when no resolver is configured.
#[derive(Debug, Default)]
pub struct NoopDefaults;

impl DependencyDefaults for NoopDefaults {
    fn apply_defaults(
        &self,
        _manifest: &mut Manifest,
        _opts: &DefaultOptions<'_>,
    ) -> Result<ResolveReport, AdepError> {
        Ok(ResolveReport::default())
    }
}

/// Apply dependency defaults using the provided resolver, mapping warnings.
pub fn apply_dependency_defaults(
    manifest: &mut Manifest,
    opts: &DefaultOptions<'_>,
) -> (Vec<ManifestWarning>, bool) {
    let noop = NoopDefaults;
    let resolver: &dyn DependencyDefaults = match opts.dependency_defaults {
        Some(resolver) => resolver,
        None => &noop,
    };

    match resolver.apply_defaults(manifest, opts) {
        Ok(report) => (
            report
                .warnings
                .into_iter()
                .map(|w| ManifestWarning::new(w.code, w.message))
                .collect(),
            report.mutated,
        ),
        Err(err) => (
            vec![ManifestWarning::new(
                err.code().to_string(),
                err.message().to_string(),
            )],
            false,
        ),
    }
}
