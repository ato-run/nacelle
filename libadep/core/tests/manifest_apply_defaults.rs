use libadep_core::deps::{DependencyDefaults, ResolveReport, ResolveWarning};
use libadep_core::error::AdepError;
use libadep_core::manifest::{apply_defaults, DefaultOptions, Manifest};

#[test]
fn apply_defaults_sets_profile_and_marks_mutation() {
    let mut manifest = Manifest::template(Some("0.1.0".into()), Some("stable".into()));
    manifest.pack.profile = None;

    let report = apply_defaults(&mut manifest, DefaultOptions::default());

    assert!(report.mutated);
    assert_eq!(
        manifest.pack.profile.as_deref(),
        Some("dist+cas"),
        "pack.profile should be defaulted"
    );
    assert_eq!(report.warnings.len(), 1);
    assert_eq!(report.warnings[0].code, "PACK-PROFILE-DEFAULTED");
}

#[test]
fn apply_defaults_preserves_custom_profile_with_warning() {
    let mut manifest = Manifest::template(Some("0.1.0".into()), Some("stable".into()));
    manifest.pack.profile = Some("custom-profile".into());

    let report = apply_defaults(&mut manifest, DefaultOptions::default());

    assert!(!report.mutated);
    assert_eq!(
        manifest.pack.profile.as_deref(),
        Some("custom-profile"),
        "custom profile should remain untouched"
    );
    assert_eq!(report.warnings.len(), 1);
    assert_eq!(report.warnings[0].code, "PACK-PROFILE-MISMATCH");
}

#[test]
fn apply_defaults_respects_override_value() {
    let mut manifest = Manifest::template(Some("0.1.0".into()), Some("stable".into()));
    manifest.pack.profile = None;

    let opts = DefaultOptions {
        default_pack_profile: "custom-default",
        dependency_defaults: None,
    };
    let report = apply_defaults(&mut manifest, opts);

    assert!(report.mutated);
    assert_eq!(
        manifest.pack.profile.as_deref(),
        Some("custom-default"),
        "override should be respected"
    );
}

struct WarningDefaults;

impl DependencyDefaults for WarningDefaults {
    fn apply_defaults(
        &self,
        _manifest: &mut Manifest,
        _opts: &DefaultOptions<'_>,
    ) -> Result<ResolveReport, AdepError> {
        Ok(ResolveReport::new(
            vec![ResolveWarning {
                code: "DEPS-MISSING-CAPSULE".into(),
                message: "dependency capsule reference missing".into(),
            }],
            false,
        ))
    }
}

struct ErrorDefaults;

impl DependencyDefaults for ErrorDefaults {
    fn apply_defaults(
        &self,
        _manifest: &mut Manifest,
        _opts: &DefaultOptions<'_>,
    ) -> Result<ResolveReport, AdepError> {
        Err(AdepError::new(
            "DEPS-INVALID-URI",
            "capsule reference must be oci:// or file://",
        ))
    }
}

#[test]
fn apply_defaults_includes_dependency_warnings() {
    let mut manifest = Manifest::template(Some("0.1.0".into()), Some("stable".into()));
    let resolver = WarningDefaults;
    let opts = DefaultOptions {
        default_pack_profile: "dist+cas",
        dependency_defaults: Some(&resolver),
    };

    let report = apply_defaults(&mut manifest, opts);

    assert!(report
        .warnings
        .iter()
        .any(|w| w.code == "DEPS-MISSING-CAPSULE"));
}

#[test]
fn apply_defaults_maps_dependency_errors_to_warning() {
    let mut manifest = Manifest::template(Some("0.1.0".into()), Some("stable".into()));
    let resolver = ErrorDefaults;
    let opts = DefaultOptions {
        default_pack_profile: "dist+cas",
        dependency_defaults: Some(&resolver),
    };

    let report = apply_defaults(&mut manifest, opts);

    assert!(report.warnings.iter().any(|w| w.code == "DEPS-INVALID-URI"));
}
struct MutationDefaults;

impl DependencyDefaults for MutationDefaults {
    fn apply_defaults(
        &self,
        manifest: &mut Manifest,
        _opts: &DefaultOptions<'_>,
    ) -> Result<ResolveReport, AdepError> {
        manifest
            .dep_capsules
            .push("oci://example/capsule:latest".into());
        Ok(ResolveReport::new(
            vec![ResolveWarning {
                code: "DEPS-AUTOFILL-CAPSULE".into(),
                message: "added default dependency capsule".into(),
            }],
            true,
        ))
    }
}

#[test]
fn apply_defaults_marks_mutated_when_dependencies_change() {
    let mut manifest = Manifest::template(Some("0.1.0".into()), Some("stable".into()));
    manifest.dep_capsules.clear();
    let resolver = MutationDefaults;
    let opts = DefaultOptions {
        default_pack_profile: "dist+cas",
        dependency_defaults: Some(&resolver),
    };

    let report = apply_defaults(&mut manifest, opts);

    assert!(
        report.mutated,
        "dependency defaults should mark report mutated"
    );
    assert!(manifest
        .dep_capsules
        .iter()
        .any(|capsule| capsule == "oci://example/capsule:latest"));
}
