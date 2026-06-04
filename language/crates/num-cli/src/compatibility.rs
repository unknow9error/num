use crate::package::PackageManifest;
use serde_json::{json, Value};

pub const CURRENT_LANGUAGE_VERSION: &str = "0.1.0";
pub const CURRENT_MANIFEST_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibilityReport {
    pub package_name: String,
    pub package_version: String,
    pub language_version: String,
    pub current_language_version: String,
    pub compatibility: String,
    pub manifest_schema: u32,
    pub current_manifest_schema: u32,
}

impl CompatibilityReport {
    pub fn from_manifest(manifest: &PackageManifest) -> Self {
        Self {
            package_name: manifest.project.name.clone(),
            package_version: manifest.project.version.clone(),
            language_version: manifest.language.version.clone(),
            current_language_version: CURRENT_LANGUAGE_VERSION.to_string(),
            compatibility: manifest.language.compatibility.clone(),
            manifest_schema: manifest.language.manifest_schema,
            current_manifest_schema: CURRENT_MANIFEST_SCHEMA,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "package": {
                "name": self.package_name,
                "version": self.package_version,
            },
            "language": {
                "version": self.language_version,
                "current": self.current_language_version,
                "compatibility": self.compatibility,
            },
            "manifest": {
                "schema": self.manifest_schema,
                "current_schema": self.current_manifest_schema,
            },
            "status": "compatible",
        })
    }

    pub fn render_text(&self) -> String {
        format!(
            "Compatibility OK for {} {}\n  language: {} against current {} ({})\n  manifest schema: {} against current {}\n",
            self.package_name,
            self.package_version,
            self.language_version,
            self.current_language_version,
            self.compatibility,
            self.manifest_schema,
            self.current_manifest_schema,
        )
    }
}

pub fn validate_manifest(manifest: &PackageManifest) -> Result<CompatibilityReport, String> {
    validate_manifest_schema(manifest)?;
    validate_language_version(manifest)?;
    Ok(CompatibilityReport::from_manifest(manifest))
}

fn validate_manifest_schema(manifest: &PackageManifest) -> Result<(), String> {
    let schema = manifest.language.manifest_schema;
    if schema == 0 {
        return Err(format!(
            "{} declares invalid [language].manifest_schema 0; expected 1",
            manifest.path.display()
        ));
    }
    if schema > CURRENT_MANIFEST_SCHEMA {
        return Err(format!(
            "{} requires manifest schema {}, but this num CLI supports schema {}",
            manifest.path.display(),
            schema,
            CURRENT_MANIFEST_SCHEMA
        ));
    }
    Ok(())
}

fn validate_language_version(manifest: &PackageManifest) -> Result<(), String> {
    let requested = Version::parse(&manifest.language.version).ok_or_else(|| {
        format!(
            "{} has invalid [language].version `{}`; expected x.y.z",
            manifest.path.display(),
            manifest.language.version
        )
    })?;
    let current = Version::parse(CURRENT_LANGUAGE_VERSION).expect("valid current version");

    let compatible = match manifest.language.compatibility.as_str() {
        "exact" => requested == current,
        "minor" => {
            requested.major == current.major
                && requested.minor == current.minor
                && requested.patch <= current.patch
        }
        "major" => requested.major == current.major && requested <= current,
        other => {
            return Err(format!(
                "{} has invalid [language].compatibility `{other}`; expected `exact`, `minor`, or `major`",
                manifest.path.display()
            ))
        }
    };

    if compatible {
        return Ok(());
    }

    Err(format!(
        "{} requires language {} with `{}` compatibility, but this num CLI supports {}",
        manifest.path.display(),
        manifest.language.version,
        manifest.language.compatibility,
        CURRENT_LANGUAGE_VERSION
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

impl Version {
    fn parse(value: &str) -> Option<Self> {
        let mut parts = value.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn manifest(language: &str) -> PackageManifest {
        let root = Path::new("/workspace/app");
        PackageManifest::parse(
            root,
            &root.join("num.toml"),
            &format!(
                r#"
[language]
{language}

[project]
name = "app"
version = "0.1.0"
"#
            ),
        )
    }

    #[test]
    fn accepts_default_language_contract() {
        let manifest = manifest("");
        let report = validate_manifest(&manifest).unwrap();

        assert_eq!(report.language_version, CURRENT_LANGUAGE_VERSION);
        assert_eq!(report.manifest_schema, CURRENT_MANIFEST_SCHEMA);
        assert_eq!(report.to_json()["status"], "compatible");
    }

    #[test]
    fn rejects_future_language_version() {
        let manifest = manifest(
            r#"
version = "0.2.0"
compatibility = "minor"
"#,
        );

        assert!(validate_manifest(&manifest)
            .unwrap_err()
            .contains("requires language 0.2.0"));
    }

    #[test]
    fn rejects_future_manifest_schema() {
        let manifest = manifest(
            r#"
version = "0.1.0"
manifest_schema = 2
"#,
        );

        assert!(validate_manifest(&manifest)
            .unwrap_err()
            .contains("requires manifest schema 2"));
    }
}
