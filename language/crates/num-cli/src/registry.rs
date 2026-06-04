use crate::compatibility::validate_manifest;
use crate::package::{DependencySource, PackageDependency, PackageManifest};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LocalRegistry {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishReport {
    pub package_name: String,
    pub package_version: String,
    pub registry_root: PathBuf,
    pub package_root: PathBuf,
    pub files: Vec<String>,
    pub dry_run: bool,
    pub replaced: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReport {
    pub package_name: String,
    pub package_version: String,
    pub registry_root: PathBuf,
    pub source_root: PathBuf,
    pub install_root: PathBuf,
    pub files: Vec<String>,
    pub dry_run: bool,
    pub replaced: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryListReport {
    pub registry_root: PathBuf,
    pub packages: Vec<RegistryPackage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryPackage {
    pub name: String,
    pub versions: Vec<String>,
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

    pub fn publish(
        &self,
        manifest: &PackageManifest,
        dry_run: bool,
        replace: bool,
    ) -> Result<PublishReport, String> {
        validate_manifest(manifest)?;
        let package_root = self
            .root
            .join(&manifest.project.name)
            .join(&manifest.project.version);
        let files = collect_package_files(&manifest.root)?;
        if package_root.exists() && !replace {
            return Err(format!(
                "registry package {} {} already exists at {}; pass --replace to overwrite it",
                manifest.project.name,
                manifest.project.version,
                package_root.display()
            ));
        }

        if !dry_run {
            if package_root.exists() {
                fs::remove_dir_all(&package_root).map_err(|err| {
                    format!("failed to replace {}: {err}", package_root.display())
                })?;
            }
            for relative in &files {
                let from = manifest.root.join(relative);
                let to = package_root.join(relative);
                if let Some(parent) = to.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
                }
                fs::copy(&from, &to).map_err(|err| {
                    format!(
                        "failed to copy {} to {}: {err}",
                        from.display(),
                        to.display()
                    )
                })?;
            }
        }

        Ok(PublishReport {
            package_name: manifest.project.name.clone(),
            package_version: manifest.project.version.clone(),
            registry_root: self.root.clone(),
            package_root,
            files,
            dry_run,
            replaced: replace,
        })
    }

    pub fn install(
        &self,
        name: &str,
        version: &str,
        install_root: &Path,
        dry_run: bool,
        replace: bool,
    ) -> Result<InstallReport, String> {
        let dependency = PackageDependency {
            name: name.to_string(),
            version: version.to_string(),
            source: DependencySource::Registry,
        };
        let manifest = self
            .resolve(&dependency)?
            .ok_or_else(|| format!("registry dependency `{name}` version `{version}` not found"))?;
        validate_manifest(&manifest)?;
        let target_root = install_root.join(name).join(version);
        let files = collect_package_files(&manifest.root)?;
        if target_root.exists() && !replace {
            return Err(format!(
                "installed package {name} {version} already exists at {}; pass --replace to overwrite it",
                target_root.display()
            ));
        }

        if !dry_run {
            if target_root.exists() {
                fs::remove_dir_all(&target_root)
                    .map_err(|err| format!("failed to replace {}: {err}", target_root.display()))?;
            }
            for relative in &files {
                let from = manifest.root.join(relative);
                let to = target_root.join(relative);
                if let Some(parent) = to.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
                }
                fs::copy(&from, &to).map_err(|err| {
                    format!(
                        "failed to copy {} to {}: {err}",
                        from.display(),
                        to.display()
                    )
                })?;
            }
        }

        Ok(InstallReport {
            package_name: name.to_string(),
            package_version: version.to_string(),
            registry_root: self.root.clone(),
            source_root: manifest.root,
            install_root: target_root,
            files,
            dry_run,
            replaced: replace,
        })
    }

    pub fn list(&self) -> Result<RegistryListReport, String> {
        if !self.root.exists() {
            return Ok(RegistryListReport {
                registry_root: self.root.clone(),
                packages: Vec::new(),
            });
        }
        let mut packages = Vec::new();
        for entry in fs::read_dir(&self.root)
            .map_err(|err| format!("failed to read registry {}: {err}", self.root.display()))?
        {
            let entry = entry.map_err(|err| format!("failed to read registry entry: {err}"))?;
            if !entry.path().is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            let mut versions = Vec::new();
            for version_entry in fs::read_dir(entry.path())
                .map_err(|err| format!("failed to read registry package `{name}`: {err}"))?
            {
                let version_entry =
                    version_entry.map_err(|err| format!("failed to read version entry: {err}"))?;
                if !version_entry.path().join("num.toml").is_file() {
                    continue;
                }
                if let Some(version) = version_entry.file_name().to_str() {
                    versions.push(version.to_string());
                }
            }
            versions.sort();
            if !versions.is_empty() {
                packages.push(RegistryPackage { name, versions });
            }
        }
        packages.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(RegistryListReport {
            registry_root: self.root.clone(),
            packages,
        })
    }
}

impl PublishReport {
    pub fn to_json(&self) -> Value {
        json!({
            "package": {
                "name": self.package_name,
                "version": self.package_version,
            },
            "registry_root": self.registry_root.display().to_string(),
            "package_root": self.package_root.display().to_string(),
            "files": self.files,
            "dry_run": self.dry_run,
            "replaced": self.replaced,
        })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Registry publish: {} {}\n",
            self.package_name, self.package_version
        ));
        out.push_str(&format!("Registry: {}\n", self.registry_root.display()));
        out.push_str(&format!("Package root: {}\n", self.package_root.display()));
        out.push_str(if self.dry_run {
            "Status: dry-run\n"
        } else {
            "Status: published\n"
        });
        out.push_str(&format!("Files: {}\n", self.files.len()));
        out
    }
}

impl InstallReport {
    pub fn to_json(&self) -> Value {
        json!({
            "package": {
                "name": self.package_name,
                "version": self.package_version,
            },
            "registry_root": self.registry_root.display().to_string(),
            "source_root": self.source_root.display().to_string(),
            "install_root": self.install_root.display().to_string(),
            "files": self.files,
            "dry_run": self.dry_run,
            "replaced": self.replaced,
        })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Registry install: {} {}\n",
            self.package_name, self.package_version
        ));
        out.push_str(&format!("Registry: {}\n", self.registry_root.display()));
        out.push_str(&format!("Install root: {}\n", self.install_root.display()));
        out.push_str(if self.dry_run {
            "Status: dry-run\n"
        } else {
            "Status: installed\n"
        });
        out.push_str(&format!("Files: {}\n", self.files.len()));
        out
    }
}

impl RegistryListReport {
    pub fn to_json(&self) -> Value {
        json!({
            "registry_root": self.registry_root.display().to_string(),
            "packages": self.packages.iter().map(RegistryPackage::to_json).collect::<Vec<_>>(),
        })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Registry: {}\n", self.registry_root.display()));
        if self.packages.is_empty() {
            out.push_str("Packages: none\n");
            return out;
        }
        out.push_str("Packages:\n");
        for package in &self.packages {
            out.push_str(&format!(
                "  - {}: {}\n",
                package.name,
                package.versions.join(", ")
            ));
        }
        out
    }
}

impl RegistryPackage {
    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "versions": self.versions,
        })
    }
}

pub fn registry_for_manifest(
    manifest: &PackageManifest,
    explicit_root: Option<PathBuf>,
) -> Result<LocalRegistry, String> {
    explicit_root
        .or_else(|| manifest.registry.path.as_ref().map(|path| manifest.root.join(path)))
        .or_else(|| std::env::var("NUM_REGISTRY_PATH").ok().map(PathBuf::from))
        .map(LocalRegistry::new)
        .ok_or_else(|| {
            "registry root is required; pass --registry <path>, set [registry].path, or set NUM_REGISTRY_PATH"
                .to_string()
        })
}

pub fn registry_from_arg(explicit_root: Option<PathBuf>) -> Result<LocalRegistry, String> {
    explicit_root
        .or_else(|| std::env::var("NUM_REGISTRY_PATH").ok().map(PathBuf::from))
        .map(LocalRegistry::new)
        .ok_or_else(|| {
            "registry root is required; pass --registry <path> or set NUM_REGISTRY_PATH".to_string()
        })
}

fn collect_package_files(root: &Path) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files(root: &Path, dir: &Path, files: &mut Vec<String>) -> Result<(), String> {
    for entry in fs::read_dir(dir)
        .map_err(|err| format!("failed to read package directory {}: {err}", dir.display()))?
    {
        let entry = entry.map_err(|err| format!("failed to read package entry: {err}"))?;
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if should_skip_entry(&name) {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, files)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|err| format!("failed to relativize {}: {err}", path.display()))?
                .to_string_lossy()
                .replace('\\', "/");
            files.push(relative);
        }
    }
    Ok(())
}

fn should_skip_entry(name: &str) -> bool {
    matches!(
        name,
        ".git" | ".DS_Store" | ".num-state" | "target" | "node_modules" | "dist"
    )
}

#[cfg(test)]
mod tests {
    use super::{registry_for_manifest, LocalRegistry};
    use crate::package::{DependencySource, PackageDependency, PackageManifest};
    use std::fs;
    use std::path::Path;
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

    fn write_package(root: &Path, name: &str, version: &str) {
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("num.toml"),
            format!(
                r#"[language]
version = "0.1.0"
compatibility = "minor"
manifest_schema = 1

[project]
name = "{name}"
version = "{version}"
source = "src"
entry = "src/lib.num"
"#
            ),
        )
        .unwrap();
        fs::write(root.join("src/lib.num"), "module shared.lib\n").unwrap();
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

    #[test]
    fn publishes_package_to_registry() {
        let package_root = temp_registry_dir("publish_package");
        let registry_root = temp_registry_dir("publish_registry");
        write_package(&package_root, "shared", "1.2.3");
        fs::create_dir_all(package_root.join("target")).unwrap();
        fs::write(package_root.join("target/build.log"), "ignore").unwrap();
        let manifest = PackageManifest::discover(&package_root).unwrap().unwrap();
        let registry = LocalRegistry::new(&registry_root);

        let report = registry.publish(&manifest, false, false).unwrap();

        assert_eq!(report.package_name, "shared");
        assert!(registry_root
            .join("shared")
            .join("1.2.3")
            .join("num.toml")
            .is_file());
        assert!(registry_root
            .join("shared")
            .join("1.2.3")
            .join("src/lib.num")
            .is_file());
        assert!(!registry_root
            .join("shared")
            .join("1.2.3")
            .join("target/build.log")
            .exists());
        fs::remove_dir_all(package_root).unwrap();
        fs::remove_dir_all(registry_root).unwrap();
    }

    #[test]
    fn install_copies_package_from_registry() {
        let registry_root = temp_registry_dir("install_registry");
        let package_root = registry_root.join("shared").join("1.2.3");
        write_package(&package_root, "shared", "1.2.3");
        let install_root = temp_registry_dir("install_target");
        let registry = LocalRegistry::new(&registry_root);

        let report = registry
            .install("shared", "1.2.3", &install_root, false, false)
            .unwrap();

        assert_eq!(report.package_name, "shared");
        assert!(install_root
            .join("shared")
            .join("1.2.3")
            .join("num.toml")
            .is_file());
        fs::remove_dir_all(registry_root).unwrap();
        fs::remove_dir_all(install_root).unwrap();
    }

    #[test]
    fn lists_registry_packages() {
        let registry_root = temp_registry_dir("list");
        write_package(
            &registry_root.join("shared").join("1.2.3"),
            "shared",
            "1.2.3",
        );
        write_package(
            &registry_root.join("shared").join("1.3.0"),
            "shared",
            "1.3.0",
        );
        write_package(&registry_root.join("core").join("0.1.0"), "core", "0.1.0");
        let registry = LocalRegistry::new(&registry_root);

        let report = registry.list().unwrap();

        assert_eq!(report.packages[0].name, "core");
        assert_eq!(report.packages[1].versions, vec!["1.2.3", "1.3.0"]);
        assert_eq!(report.to_json()["packages"][0]["name"], "core");
        assert!(report.render_text().contains("shared: 1.2.3, 1.3.0"));
        fs::remove_dir_all(registry_root).unwrap();
    }

    #[test]
    fn registry_for_manifest_prefers_explicit_root() {
        let package_root = temp_registry_dir("registry_root_manifest");
        write_package(&package_root, "shared", "1.2.3");
        let manifest = PackageManifest::discover(&package_root).unwrap().unwrap();
        let explicit = package_root.join("explicit-registry");

        let registry = registry_for_manifest(&manifest, Some(explicit.clone())).unwrap();
        let report = registry.list().unwrap();

        assert_eq!(report.registry_root, explicit);
        fs::remove_dir_all(package_root).unwrap();
    }
}
