use crate::compatibility::validate_manifest;
use crate::integrity::{bytes_hash, ContentHasher};
use crate::package::{DependencySource, PackageDependency, PackageManifest};
use serde_json::{json, Value};
use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};

const REGISTRY_METADATA_FILE: &str = ".num-package.json";
const REGISTRY_METADATA_SCHEMA: u32 = 1;

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
    pub metadata_path: PathBuf,
    pub content_hash: String,
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
    pub metadata_path: PathBuf,
    pub content_hash: String,
    pub dry_run: bool,
    pub replaced: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryListReport {
    pub registry_root: PathBuf,
    pub packages: Vec<RegistryPackage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryIndexReport {
    pub schema: u32,
    pub registry_root: PathBuf,
    pub packages: Vec<RegistryIndexPackage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryPackage {
    pub name: String,
    pub versions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryIndexPackage {
    pub name: String,
    pub versions: Vec<RegistryIndexVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryIndexVersion {
    pub version: String,
    pub language: String,
    pub manifest_schema: u32,
    pub content_hash: String,
    pub file_count: usize,
    pub metadata_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryPackageResolution {
    pub manifest: PackageManifest,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegistryPackageMetadata {
    schema: u32,
    name: String,
    version: String,
    language: String,
    manifest_schema: u32,
    files: Vec<RegistryPackageFile>,
    content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegistryPackageFile {
    path: String,
    size: u64,
    hash: String,
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
        Ok(self
            .resolve_with_metadata(dependency)?
            .map(|resolved| resolved.manifest))
    }

    pub fn resolve_with_metadata(
        &self,
        dependency: &PackageDependency,
    ) -> Result<Option<RegistryPackageResolution>, String> {
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
        let metadata = validate_registry_metadata(&package_root, &manifest)?;
        Ok(Some(RegistryPackageResolution {
            manifest,
            content_hash: metadata.content_hash,
        }))
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
        let metadata = RegistryPackageMetadata::from_package(manifest, &files)?;
        let metadata_path = package_root.join(REGISTRY_METADATA_FILE);
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
            metadata.write_to(&metadata_path)?;
        }

        Ok(PublishReport {
            package_name: manifest.project.name.clone(),
            package_version: manifest.project.version.clone(),
            registry_root: self.root.clone(),
            package_root,
            files,
            metadata_path,
            content_hash: metadata.content_hash,
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
        let package_version = if version == "latest" {
            self.latest_version(name)?
        } else {
            version.to_string()
        };
        let dependency = PackageDependency {
            name: name.to_string(),
            version: package_version.clone(),
            source: DependencySource::Registry,
        };
        let manifest = self.resolve(&dependency)?.ok_or_else(|| {
            format!("registry dependency `{name}` version `{package_version}` not found")
        })?;
        validate_manifest(&manifest)?;
        let metadata = validate_registry_metadata(&manifest.root, &manifest)?;
        let target_root = install_root.join(name).join(&package_version);
        let files = collect_package_files(&manifest.root)?;
        let metadata_path = target_root.join(REGISTRY_METADATA_FILE);
        if target_root.exists() && !replace {
            return Err(format!(
                "installed package {name} {package_version} already exists at {}; pass --replace to overwrite it",
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
            metadata.write_to(&metadata_path)?;
        }

        Ok(InstallReport {
            package_name: name.to_string(),
            package_version,
            registry_root: self.root.clone(),
            source_root: manifest.root,
            install_root: target_root,
            files,
            metadata_path,
            content_hash: metadata.content_hash,
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
            sort_registry_versions(&mut versions);
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

    pub fn index(&self) -> Result<RegistryIndexReport, String> {
        if !self.root.exists() {
            return Ok(RegistryIndexReport {
                schema: REGISTRY_METADATA_SCHEMA,
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
                let package_root = version_entry.path();
                let manifest_path = package_root.join("num.toml");
                if !manifest_path.is_file() {
                    continue;
                }
                let source = fs::read_to_string(&manifest_path)
                    .map_err(|err| format!("failed to read {}: {err}", manifest_path.display()))?;
                let manifest = PackageManifest::parse(&package_root, &manifest_path, &source);
                let metadata = validate_registry_metadata(&package_root, &manifest)?;
                versions.push(RegistryIndexVersion {
                    version: metadata.version,
                    language: metadata.language,
                    manifest_schema: metadata.manifest_schema,
                    content_hash: metadata.content_hash,
                    file_count: metadata.files.len(),
                    metadata_path: package_root.join(REGISTRY_METADATA_FILE),
                });
            }
            versions
                .sort_by(|left, right| compare_registry_versions(&left.version, &right.version));
            if !versions.is_empty() {
                packages.push(RegistryIndexPackage { name, versions });
            }
        }
        packages.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(RegistryIndexReport {
            schema: REGISTRY_METADATA_SCHEMA,
            registry_root: self.root.clone(),
            packages,
        })
    }

    fn latest_version(&self, name: &str) -> Result<String, String> {
        let package_root = self.root.join(name);
        if !package_root.is_dir() {
            return Err(format!(
                "registry package `{name}` was not found at {}",
                package_root.display()
            ));
        }

        let mut versions = Vec::new();
        for entry in fs::read_dir(&package_root)
            .map_err(|err| format!("failed to read registry package `{name}`: {err}"))?
        {
            let entry = entry.map_err(|err| format!("failed to read version entry: {err}"))?;
            if !entry.path().join("num.toml").is_file() {
                continue;
            }
            if let Some(version) = entry.file_name().to_str() {
                if RegistrySemVer::parse(version).is_some() {
                    versions.push(version.to_string());
                }
            }
        }
        sort_registry_versions(&mut versions);
        versions
            .pop()
            .ok_or_else(|| format!("registry package `{name}` has no SemVer-compatible versions"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegistrySemVer {
    major: u64,
    minor: u64,
    patch: u64,
    pre: Vec<PrereleaseIdentifier>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PrereleaseIdentifier {
    Numeric(u64),
    Text(String),
}

impl RegistrySemVer {
    fn parse(version: &str) -> Option<Self> {
        let public = version.split_once('+').map_or(version, |(left, _)| left);
        let (core, pre) = public
            .split_once('-')
            .map_or((public, ""), |(core, pre)| (core, pre));
        let mut parts = core.split('.');
        let major = parse_numeric_identifier(parts.next()?)?;
        let minor = parse_numeric_identifier(parts.next()?)?;
        let patch = parse_numeric_identifier(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }
        let pre = if pre.is_empty() {
            Vec::new()
        } else {
            pre.split('.')
                .map(parse_prerelease_identifier)
                .collect::<Option<Vec<_>>>()?
        };
        Some(Self {
            major,
            minor,
            patch,
            pre,
        })
    }
}

impl Ord for RegistrySemVer {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch))
            .then_with(|| compare_prerelease(&self.pre, &other.pre))
    }
}

impl PartialOrd for RegistrySemVer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PrereleaseIdentifier {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Numeric(left), Self::Numeric(right)) => left.cmp(right),
            (Self::Numeric(_), Self::Text(_)) => Ordering::Less,
            (Self::Text(_), Self::Numeric(_)) => Ordering::Greater,
            (Self::Text(left), Self::Text(right)) => left.cmp(right),
        }
    }
}

impl PartialOrd for PrereleaseIdentifier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn parse_numeric_identifier(raw: &str) -> Option<u64> {
    if raw.is_empty() || (raw.len() > 1 && raw.starts_with('0')) {
        return None;
    }
    raw.parse().ok()
}

fn parse_prerelease_identifier(raw: &str) -> Option<PrereleaseIdentifier> {
    if raw.is_empty() {
        return None;
    }
    if raw.chars().all(|ch| ch.is_ascii_digit()) {
        parse_numeric_identifier(raw).map(PrereleaseIdentifier::Numeric)
    } else if raw
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    {
        Some(PrereleaseIdentifier::Text(raw.to_string()))
    } else {
        None
    }
}

fn compare_prerelease(left: &[PrereleaseIdentifier], right: &[PrereleaseIdentifier]) -> Ordering {
    match (left.is_empty(), right.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => left.cmp(right),
    }
}

fn compare_registry_versions(left: &str, right: &str) -> Ordering {
    match (RegistrySemVer::parse(left), RegistrySemVer::parse(right)) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => left.cmp(right),
    }
}

fn sort_registry_versions(versions: &mut [String]) {
    versions.sort_by(|left, right| {
        compare_registry_versions(left, right).then_with(|| left.cmp(right))
    });
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
            "metadata_path": self.metadata_path.display().to_string(),
            "content_hash": self.content_hash,
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
        out.push_str(&format!("Metadata: {}\n", self.metadata_path.display()));
        out.push_str(&format!("Content hash: {}\n", self.content_hash));
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
            "metadata_path": self.metadata_path.display().to_string(),
            "content_hash": self.content_hash,
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
        out.push_str(&format!("Metadata: {}\n", self.metadata_path.display()));
        out.push_str(&format!("Content hash: {}\n", self.content_hash));
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

impl RegistryIndexReport {
    pub fn to_json(&self) -> Value {
        json!({
            "schema": self.schema,
            "registry_root": self.registry_root.display().to_string(),
            "packages": self.packages.iter().map(RegistryIndexPackage::to_json).collect::<Vec<_>>(),
        })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Registry index schema: {}\n", self.schema));
        out.push_str(&format!("Registry: {}\n", self.registry_root.display()));
        if self.packages.is_empty() {
            out.push_str("Packages: none\n");
            return out;
        }
        out.push_str("Packages:\n");
        for package in &self.packages {
            out.push_str(&format!("  - {}\n", package.name));
            for version in &package.versions {
                out.push_str(&format!(
                    "      {} language={} manifest_schema={} hash={} files={}\n",
                    version.version,
                    version.language,
                    version.manifest_schema,
                    version.content_hash,
                    version.file_count
                ));
            }
        }
        out
    }
}

impl RegistryIndexPackage {
    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "versions": self.versions.iter().map(RegistryIndexVersion::to_json).collect::<Vec<_>>(),
        })
    }
}

impl RegistryIndexVersion {
    fn to_json(&self) -> Value {
        json!({
            "version": self.version,
            "language": self.language,
            "manifest_schema": self.manifest_schema,
            "content_hash": self.content_hash,
            "file_count": self.file_count,
            "metadata_path": self.metadata_path.display().to_string(),
        })
    }
}

impl RegistryPackageMetadata {
    fn from_package(manifest: &PackageManifest, files: &[String]) -> Result<Self, String> {
        let mut entries = Vec::new();
        let mut package_hash = ContentHasher::new();
        for relative in files {
            let path = manifest.root.join(relative);
            let bytes = fs::read(&path)
                .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
            let file_hash = bytes_hash(&bytes);
            package_hash.update(relative.as_bytes());
            package_hash.update(&[0]);
            package_hash.update(file_hash.as_bytes());
            package_hash.update(&[0]);
            entries.push(RegistryPackageFile {
                path: relative.clone(),
                size: bytes.len() as u64,
                hash: file_hash,
            });
        }

        Ok(Self {
            schema: REGISTRY_METADATA_SCHEMA,
            name: manifest.project.name.clone(),
            version: manifest.project.version.clone(),
            language: manifest.language.version.clone(),
            manifest_schema: manifest.language.manifest_schema,
            files: entries,
            content_hash: package_hash.finish(),
        })
    }

    fn read_from(path: &Path) -> Result<Option<Self>, String> {
        if !path.is_file() {
            return Ok(None);
        }
        let source = fs::read_to_string(path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        let value: Value = serde_json::from_str(&source)
            .map_err(|err| format!("failed to parse {}: {err}", path.display()))?;
        let schema = json_u32(&value, "schema")?;
        let name = json_string(&value, "name")?;
        let version = json_string(&value, "version")?;
        let language = json_string(&value, "language")?;
        let manifest_schema = json_u32(&value, "manifest_schema")?;
        let content_hash = json_string(&value, "content_hash")?;
        let files_value = value
            .get("files")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("{} is missing array field `files`", path.display()))?;
        let mut files = Vec::new();
        for file in files_value {
            let path = json_string(file, "path")?;
            let size = file.get("size").and_then(Value::as_u64).ok_or_else(|| {
                "registry metadata file entry is missing numeric `size`".to_string()
            })?;
            let hash = json_string(file, "hash")?;
            files.push(RegistryPackageFile { path, size, hash });
        }

        Ok(Some(Self {
            schema,
            name,
            version,
            language,
            manifest_schema,
            files,
            content_hash,
        }))
    }

    fn write_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
        }
        fs::write(path, self.to_json_string())
            .map_err(|err| format!("failed to write {}: {err}", path.display()))
    }

    fn to_json_string(&self) -> String {
        let value = json!({
            "schema": self.schema,
            "name": self.name,
            "version": self.version,
            "language": self.language,
            "manifest_schema": self.manifest_schema,
            "content_hash": self.content_hash,
            "files": self.files.iter().map(RegistryPackageFile::to_json).collect::<Vec<_>>(),
        });
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()) + "\n"
    }
}

impl RegistryPackageFile {
    fn to_json(&self) -> Value {
        json!({
            "path": self.path,
            "size": self.size,
            "hash": self.hash,
        })
    }
}

fn validate_registry_metadata(
    package_root: &Path,
    manifest: &PackageManifest,
) -> Result<RegistryPackageMetadata, String> {
    let metadata_path = package_root.join(REGISTRY_METADATA_FILE);
    let files = collect_package_files(package_root)?;
    let actual = RegistryPackageMetadata::from_package(manifest, &files)?;
    let Some(recorded) = RegistryPackageMetadata::read_from(&metadata_path)? else {
        return Ok(actual);
    };

    if recorded.schema != REGISTRY_METADATA_SCHEMA {
        return Err(format!(
            "{} declares unsupported registry metadata schema {}; expected {}",
            metadata_path.display(),
            recorded.schema,
            REGISTRY_METADATA_SCHEMA
        ));
    }
    if recorded.name != actual.name
        || recorded.version != actual.version
        || recorded.language != actual.language
        || recorded.manifest_schema != actual.manifest_schema
    {
        return Err(format!(
            "{} does not match package manifest {} {}",
            metadata_path.display(),
            manifest.project.name,
            manifest.project.version
        ));
    }
    if recorded.files != actual.files || recorded.content_hash != actual.content_hash {
        return Err(format!(
            "{} content hash does not match package files",
            metadata_path.display()
        ));
    }

    Ok(recorded)
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
        REGISTRY_METADATA_FILE
            | ".git"
            | ".DS_Store"
            | ".num-state"
            | "target"
            | "node_modules"
            | "dist"
    )
}

fn json_string(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("registry metadata is missing string field `{field}`"))
}

fn json_u32(value: &Value, field: &str) -> Result<u32, String> {
    let raw = value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("registry metadata is missing numeric field `{field}`"))?;
    u32::try_from(raw).map_err(|_| format!("registry metadata field `{field}` is too large"))
}

#[cfg(test)]
mod tests {
    use super::{registry_for_manifest, LocalRegistry, REGISTRY_METADATA_FILE};
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
version = "0.4.0"
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
        let resolved = registry
            .resolve_with_metadata(&dependency)
            .unwrap()
            .unwrap();

        assert_eq!(manifest.project.name, "shared");
        assert_eq!(resolved.manifest.project.name, "shared");
        assert!(resolved.content_hash.starts_with("sha256:"));
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
        assert!(registry_root
            .join("shared")
            .join("1.2.3")
            .join(REGISTRY_METADATA_FILE)
            .is_file());
        assert!(report.content_hash.starts_with("sha256:"));
        assert_eq!(
            report.metadata_path,
            registry_root
                .join("shared")
                .join("1.2.3")
                .join(REGISTRY_METADATA_FILE)
        );
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
        assert!(install_root
            .join("shared")
            .join("1.2.3")
            .join(REGISTRY_METADATA_FILE)
            .is_file());
        assert!(report.content_hash.starts_with("sha256:"));
        fs::remove_dir_all(registry_root).unwrap();
        fs::remove_dir_all(install_root).unwrap();
    }

    #[test]
    fn install_latest_resolves_highest_semver_registry_version() {
        let registry_root = temp_registry_dir("install_latest_registry");
        write_package(
            &registry_root.join("shared").join("1.2.10"),
            "shared",
            "1.2.10",
        );
        write_package(
            &registry_root.join("shared").join("1.10.0-alpha.1"),
            "shared",
            "1.10.0-alpha.1",
        );
        write_package(
            &registry_root.join("shared").join("1.10.0"),
            "shared",
            "1.10.0",
        );
        let install_root = temp_registry_dir("install_latest_target");
        let registry = LocalRegistry::new(&registry_root);

        let report = registry
            .install("shared", "latest", &install_root, false, false)
            .unwrap();

        assert_eq!(report.package_version, "1.10.0");
        assert!(install_root
            .join("shared")
            .join("1.10.0")
            .join("num.toml")
            .is_file());
        fs::remove_dir_all(registry_root).unwrap();
        fs::remove_dir_all(install_root).unwrap();
    }

    #[test]
    fn install_rejects_package_when_registry_metadata_hash_does_not_match() {
        let package_root = temp_registry_dir("integrity_package");
        let registry_root = temp_registry_dir("integrity_registry");
        let install_root = temp_registry_dir("integrity_target");
        write_package(&package_root, "shared", "1.2.3");
        let manifest = PackageManifest::discover(&package_root).unwrap().unwrap();
        let registry = LocalRegistry::new(&registry_root);
        registry.publish(&manifest, false, false).unwrap();
        fs::write(
            registry_root
                .join("shared")
                .join("1.2.3")
                .join("src/lib.num"),
            "module shared.changed\n",
        )
        .unwrap();

        let err = registry
            .install("shared", "1.2.3", &install_root, false, false)
            .unwrap_err();

        assert!(err.contains("content hash does not match package files"));
        fs::remove_dir_all(package_root).unwrap();
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
            &registry_root.join("shared").join("1.10.0"),
            "shared",
            "1.10.0",
        );
        write_package(
            &registry_root.join("shared").join("1.2.10"),
            "shared",
            "1.2.10",
        );
        write_package(&registry_root.join("core").join("0.1.0"), "core", "0.1.0");
        let registry = LocalRegistry::new(&registry_root);

        let report = registry.list().unwrap();

        assert_eq!(report.packages[0].name, "core");
        assert_eq!(
            report.packages[1].versions,
            vec!["1.2.3", "1.2.10", "1.10.0"]
        );
        assert_eq!(report.to_json()["packages"][0]["name"], "core");
        assert!(report
            .render_text()
            .contains("shared: 1.2.3, 1.2.10, 1.10.0"));
        fs::remove_dir_all(registry_root).unwrap();
    }

    #[test]
    fn indexes_registry_package_metadata() {
        let registry_root = temp_registry_dir("index_registry");
        write_package(
            &registry_root.join("shared").join("1.2.3"),
            "shared",
            "1.2.3",
        );
        write_package(
            &registry_root.join("shared").join("1.2.10"),
            "shared",
            "1.2.10",
        );
        let registry = LocalRegistry::new(&registry_root);

        let report = registry.index().unwrap();

        assert_eq!(report.schema, 1);
        assert_eq!(report.packages[0].name, "shared");
        assert_eq!(report.packages[0].versions[0].version, "1.2.3");
        assert_eq!(report.packages[0].versions[1].version, "1.2.10");
        assert_eq!(report.packages[0].versions[0].language, "0.4.0");
        assert_eq!(report.packages[0].versions[0].manifest_schema, 1);
        assert!(report.packages[0].versions[0]
            .content_hash
            .starts_with("sha256:"));
        assert_eq!(
            report.to_json()["packages"][0]["versions"][0]["version"],
            "1.2.3"
        );
        assert!(report.render_text().contains("shared"));
        assert!(report.render_text().contains("hash=sha256:"));

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
