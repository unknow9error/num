use crate::package::{DependencySource, PackageDependency, PackageManifest};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct LocalRegistry {
    root: PathBuf,
}

impl LocalRegistry {
    pub fn discover_for(manifest: &PackageManifest) -> Option<Self> {
        manifest
            .registry
            .path
            .as_ref()
            .map(|path| manifest.root.join(path))
            .or_else(|| std::env::var("NUM_REGISTRY_PATH").ok().map(PathBuf::from))
            .map(Self::new)
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn resolve(
        &self,
        dependency: &PackageDependency,
    ) -> Result<Option<PackageManifest>, String> {
        let DependencySource::Registry = dependency.source else {
            return Ok(None);
        };

        let package_root = self.root.join(&dependency.name).join(&dependency.version);
        let manifest_path = package_root.join("num.toml");
        if !manifest_path.is_file() {
            return Err(format!(
                "registry dependency `{}` version `{}` was not found at {}",
                dependency.name,
                dependency.version,
                manifest_path.display()
            ));
        }

        let source = fs::read_to_string(&manifest_path)
            .map_err(|err| format!("failed to read {}: {err}", manifest_path.display()))?;
        let mut manifest = PackageManifest::parse(&package_root, &manifest_path, &source);
        if manifest.registry.path.is_none() {
            manifest.registry.path = Some(self.root.display().to_string());
        }
        Ok(Some(manifest))
    }
}

#[cfg(test)]
mod tests {
    use super::LocalRegistry;
    use crate::package::{DependencySource, PackageDependency};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_registry_dir(name: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("num_registry_{name}_{stamp}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn resolves_package_from_filesystem_registry() {
        let registry_root = temp_registry_dir("resolve");
        let package_root = registry_root.join("shared").join("1.2.3");
        fs::create_dir_all(package_root.join("src")).unwrap();
        fs::write(
            package_root.join("num.toml"),
            r#"
[project]
name = "shared"
version = "1.2.3"
source = "src"
entry = "src/lib.num"
"#,
        )
        .unwrap();

        let registry = LocalRegistry::new(&registry_root);
        let dependency = PackageDependency {
            name: "shared".to_string(),
            version: "1.2.3".to_string(),
            source: DependencySource::Registry,
        };

        let manifest = registry.resolve(&dependency).unwrap().unwrap();

        assert_eq!(manifest.project.name, "shared");
        assert_eq!(manifest.entry_path(), package_root.join("src/lib.num"));
        fs::remove_dir_all(registry_root).unwrap();
    }

    #[test]
    fn returns_none_for_non_registry_dependency() {
        let registry = LocalRegistry::new("/tmp/num-registry");
        let dependency = PackageDependency {
            name: "shared".to_string(),
            version: "1.2.3".to_string(),
            source: DependencySource::Path("../shared".to_string()),
        };

        assert!(registry.resolve(&dependency).unwrap().is_none());
    }
}
