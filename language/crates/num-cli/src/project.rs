use crate::compatibility;
use crate::package::PackageManifest;
use num_compiler::SourceFile;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ProgramInput {
    pub files: Vec<SourceFile>,
    pub entry_source_name: Option<String>,
    pub policy_mode: Option<String>,
}

pub fn load_program_input(path: &Path) -> Result<ProgramInput, String> {
    if let Some(project) = PackageManifest::discover(path)? {
        let source_dir = project.source_dir();
        let entry_path = project.entry_path();
        let mut files = Vec::new();
        let mut visited_packages = HashSet::new();
        collect_package_files(&project, &mut visited_packages, &mut files)?;
        let entry_source_name = if path.is_file() {
            Some(path.display().to_string())
        } else if files
            .iter()
            .any(|file| file.name == entry_path.display().to_string())
        {
            Some(entry_path.display().to_string())
        } else {
            select_entry_source(&source_dir, &files)
        };
        return Ok(ProgramInput {
            files,
            entry_source_name,
            policy_mode: Some(project.security.policy_mode.clone()),
        });
    }

    if path.is_dir() {
        let files = collect_num_files(path)?;
        let entry_source_name = select_entry_source(path, &files);
        return Ok(ProgramInput {
            files,
            entry_source_name,
            policy_mode: None,
        });
    }

    if !path.is_file() {
        return Err(format!(
            "{} is not a .num file or directory",
            path.display()
        ));
    }

    let root = path.parent().unwrap_or_else(|| Path::new("."));
    let files = collect_num_files(root)?;
    Ok(ProgramInput {
        files,
        entry_source_name: Some(path.display().to_string()),
        policy_mode: None,
    })
}

pub fn load_package_manifests(path: &Path) -> Result<Vec<PackageManifest>, String> {
    let project = PackageManifest::discover(path)?
        .ok_or_else(|| format!("no num.toml found for {}", path.display()))?;
    let mut manifests = Vec::new();
    let mut visited_packages = HashSet::new();
    collect_package_manifests(&project, &mut visited_packages, &mut manifests)?;
    Ok(manifests)
}

fn collect_package_files(
    manifest: &PackageManifest,
    visited_packages: &mut HashSet<PathBuf>,
    files: &mut Vec<SourceFile>,
) -> Result<(), String> {
    let package_key = fs::canonicalize(&manifest.root).unwrap_or_else(|_| manifest.root.clone());
    if !visited_packages.insert(package_key) {
        return Ok(());
    }

    compatibility::validate_manifest(manifest)?;
    files.extend(collect_num_files(&manifest.source_dir())?);

    for dependency in dependency_manifests(manifest)? {
        collect_package_files(&dependency, visited_packages, files)?;
    }

    Ok(())
}

fn collect_package_manifests(
    manifest: &PackageManifest,
    visited_packages: &mut HashSet<PathBuf>,
    manifests: &mut Vec<PackageManifest>,
) -> Result<(), String> {
    let package_key = fs::canonicalize(&manifest.root).unwrap_or_else(|_| manifest.root.clone());
    if !visited_packages.insert(package_key) {
        return Ok(());
    }

    compatibility::validate_manifest(manifest)?;
    manifests.push(manifest.clone());

    for dependency in dependency_manifests(manifest)? {
        collect_package_manifests(&dependency, visited_packages, manifests)?;
    }

    Ok(())
}

fn dependency_manifests(manifest: &PackageManifest) -> Result<Vec<PackageManifest>, String> {
    manifest.resolved_dependency_manifests()
}

pub fn create_project(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path.join("src"))
        .map_err(|err| format!("failed to create {}: {err}", path.display()))?;
    let manifest = format!(
        "[language]\nversion = \"{}\"\ncompatibility = \"minor\"\nmanifest_schema = {}\n\n[project]\nname = \"new-num-service\"\nversion = \"0.1.0\"\nsource = \"src\"\nentry = \"src/main.num\"\n\n[dependencies]\n\n[runtime]\nworkflow_store = \"memory\"\naudit_store = \"stdout\"\n\n[security]\npolicy_mode = \"strict\"\n",
        compatibility::CURRENT_LANGUAGE_VERSION,
        compatibility::CURRENT_MANIFEST_SCHEMA
    );
    write_file(&path.join("num.toml"), &manifest)?;
    write_file(
        &path.join("src/access.num"),
        "module app.access\n\npermission Execute\n\nrole Operator {\n    allow Execute\n}\n",
    )?;
    write_file(
        &path.join("src/domain.num"),
        "module app.domain\n\ntype StartupEvent {\n    message: Text from UserInput internal\n}\n\nfn startup_message() -> Text {\n    return \"app_started\"\n}\n",
    )?;
    write_file(
        &path.join("src/connectors.num"),
        "module app.connectors\n\nconnector audit_sink {\n    record(message: Text) -> Unit\n}\n",
    )?;
    write_file(
        &path.join("src/main.num"),
        "module app.main\n\nuse app.access\nuse app.domain\nuse app.connectors\n\nworkflow main() {\n    require Permission.Execute for current_user\n\n    let message: Text = startup_message()\n    audit(message)\n}\n",
    )?;
    println!("created num project at {}", path.display());
    Ok(())
}

fn select_entry_source(root: &Path, files: &[SourceFile]) -> Option<String> {
    for candidate in [root.join("main.num"), root.join("src/main.num")] {
        let candidate = candidate.display().to_string();
        if files.iter().any(|file| file.name == candidate) {
            return Some(candidate);
        }
    }

    files.first().map(|file| file.name.clone())
}

fn collect_num_files(path: &Path) -> Result<Vec<SourceFile>, String> {
    let mut paths = Vec::new();
    collect_num_file_paths(path, &mut paths)?;
    paths.sort();

    if paths.is_empty() {
        return Err(format!("no .num files found under {}", path.display()));
    }

    paths
        .into_iter()
        .map(|path| {
            let source = fs::read_to_string(&path)
                .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
            Ok(SourceFile::new(path.display().to_string(), source))
        })
        .collect()
}

fn collect_num_file_paths(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in
        fs::read_dir(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?
    {
        let entry =
            entry.map_err(|err| format!("failed to read {} entry: {err}", path.display()))?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            collect_num_file_paths(&entry_path, files)?;
        } else if entry_path
            .extension()
            .is_some_and(|extension| extension == "num")
        {
            files.push(entry_path);
        }
    }
    Ok(())
}

fn write_file(path: &Path, contents: &str) -> Result<(), String> {
    fs::write(path, contents).map_err(|err| format!("failed to write {}: {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_compiler::check_program;
    use std::env;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_project_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("num_cli_{name}_{stamp}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn file_path_uses_parent_manifest_source_dir() {
        let root = temp_project_dir("manifest_source");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"
source = "src"
entry = "src/main.num"
"#,
        )
        .unwrap();
        fs::write(
            root.join("src/domain.num"),
            "module app.domain\n\ntype Event {\n    message: Text\n}\n",
        )
        .unwrap();
        fs::write(
            root.join("src/main.num"),
            "module app.main\n\nuse app.domain\n\nworkflow main(event: Event) {\n    audit(event.message)\n}\n",
        )
        .unwrap();

        let input = load_program_input(&root.join("src/main.num")).unwrap();

        assert_eq!(input.files.len(), 2);
        assert_eq!(
            input.entry_source_name,
            Some(root.join("src/main.num").display().to_string())
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_path_dependency_modules_are_part_of_program_input() {
        let root = temp_project_dir("path_dependency_imports");
        let shared = root.with_file_name(format!(
            "{}_shared",
            root.file_name().unwrap().to_string_lossy()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(shared.join("src")).unwrap();
        fs::write(
            root.join("num.toml"),
            format!(
                r#"
[project]
name = "app"
version = "0.1.0"
source = "src"
entry = "src/main.num"

[dependencies]
shared = {{ path = "{}", version = "0.2.0" }}
"#,
                path_to_toml_string(shared.display().to_string())
            ),
        )
        .unwrap();
        fs::write(
            root.join("src/main.num"),
            "module app.main\n\nuse shared.domain\n\nworkflow main(event: SharedEvent) {\n    audit(event.message)\n}\n",
        )
        .unwrap();
        fs::write(
            shared.join("num.toml"),
            r#"
[project]
name = "shared"
version = "0.2.0"
source = "src"
entry = "src/domain.num"
"#,
        )
        .unwrap();
        fs::write(
            shared.join("src/domain.num"),
            "module shared.domain\n\ntype SharedEvent {\n    message: Text\n}\n",
        )
        .unwrap();

        let input = load_program_input(&root).unwrap();

        assert_eq!(input.files.len(), 2);
        assert!(input
            .files
            .iter()
            .any(|file| file.name == shared.join("src/domain.num").display().to_string()));
        assert_eq!(
            input.entry_source_name,
            Some(root.join("src/main.num").display().to_string())
        );
        let check = check_program(input.files);
        assert!(check.diagnostics.is_empty());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(shared).unwrap();
    }

    #[test]
    fn manifest_registry_dependency_modules_are_part_of_program_input() {
        let root = temp_project_dir("registry_dependency_imports");
        let registry = root.with_file_name(format!(
            "{}_registry",
            root.file_name().unwrap().to_string_lossy()
        ));
        let shared = registry.join("shared").join("0.2.0");
        let core = registry.join("core").join("1.0.0");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(shared.join("src")).unwrap();
        fs::create_dir_all(core.join("src")).unwrap();
        fs::write(
            root.join("num.toml"),
            format!(
                r#"
[project]
name = "app"
version = "0.1.0"
source = "src"
entry = "src/main.num"

[registry]
path = "{}"

[dependencies]
shared = "0.2.0"
"#,
                path_to_toml_string(registry.display().to_string())
            ),
        )
        .unwrap();
        fs::write(
            root.join("src/main.num"),
            "module app.main\n\nuse shared.domain\n\nworkflow main(event: SharedEvent) {\n    audit(event.message)\n}\n",
        )
        .unwrap();
        fs::write(
            shared.join("num.toml"),
            r#"
[project]
name = "shared"
version = "0.2.0"
source = "src"
entry = "src/domain.num"

[dependencies]
core = "1.0.0"
"#,
        )
        .unwrap();
        fs::write(
            shared.join("src/domain.num"),
            "module shared.domain\n\nuse core.types\n\ntype SharedEvent {\n    message: CoreMessage\n}\n",
        )
        .unwrap();
        fs::write(
            core.join("num.toml"),
            r#"
[project]
name = "core"
version = "1.0.0"
source = "src"
entry = "src/types.num"
"#,
        )
        .unwrap();
        fs::write(
            core.join("src/types.num"),
            "module core.types\n\ntype CoreMessage = Text\n",
        )
        .unwrap();

        let input = load_program_input(&root).unwrap();

        assert_eq!(input.files.len(), 3);
        assert!(input
            .files
            .iter()
            .any(|file| file.name == shared.join("src/domain.num").display().to_string()));
        assert!(input
            .files
            .iter()
            .any(|file| file.name == core.join("src/types.num").display().to_string()));
        let check = check_program(input.files);
        assert!(check.diagnostics.is_empty());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(registry).unwrap();
    }

    #[test]
    fn manifest_git_dependency_modules_are_part_of_program_input() {
        let root = temp_project_dir("git_dependency_imports");
        let shared = root.with_file_name(format!(
            "{}_shared_git",
            root.file_name().unwrap().to_string_lossy()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(shared.join("src")).unwrap();
        fs::write(
            shared.join("num.toml"),
            r#"
[project]
name = "shared"
version = "0.2.0"
source = "src"
entry = "src/domain.num"
"#,
        )
        .unwrap();
        fs::write(
            shared.join("src/domain.num"),
            "module shared.domain\n\ntype SharedEvent {\n    message: Text\n}\n",
        )
        .unwrap();
        init_git_repo(&shared);
        let rev = git_head_rev(&shared);
        fs::write(
            root.join("num.toml"),
            format!(
                r#"
[project]
name = "app"
version = "0.1.0"
source = "src"
entry = "src/main.num"

[dependencies]
shared = {{ git = "{}", version = "0.2.0", rev = "{}" }}
"#,
                path_to_toml_string(shared.display().to_string()),
                rev
            ),
        )
        .unwrap();
        fs::write(
            root.join("src/main.num"),
            "module app.main\n\nuse shared.domain\n\nworkflow main(event: SharedEvent) {\n    audit(event.message)\n}\n",
        )
        .unwrap();

        let input = load_program_input(&root).unwrap();

        assert_eq!(input.files.len(), 2);
        assert!(input
            .files
            .iter()
            .any(|file| file.name.contains(".num-git") && file.name.ends_with("src/domain.num")));
        let check = check_program(input.files);
        assert!(check.diagnostics.is_empty());
        assert!(root.join(".num-git").is_dir());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(shared).unwrap();
    }

    #[test]
    fn rejects_incompatible_dependency_language_version() {
        let root = temp_project_dir("incompatible_dependency");
        let shared = root.with_file_name(format!(
            "{}_shared",
            root.file_name().unwrap().to_string_lossy()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(shared.join("src")).unwrap();
        fs::write(
            root.join("num.toml"),
            format!(
                r#"
[project]
name = "app"
version = "0.1.0"
source = "src"
entry = "src/main.num"

[dependencies]
shared = {{ path = "{}", version = "0.2.0" }}
"#,
                path_to_toml_string(shared.display().to_string())
            ),
        )
        .unwrap();
        fs::write(
            root.join("src/main.num"),
            "module app.main\n\nuse shared.domain\n\nworkflow main(event: SharedEvent) {\n    audit(event.message)\n}\n",
        )
        .unwrap();
        fs::write(
            shared.join("num.toml"),
            r#"
[language]
version = "0.2.0"
compatibility = "minor"

[project]
name = "shared"
version = "0.2.0"
source = "src"
entry = "src/domain.num"
"#,
        )
        .unwrap();
        fs::write(
            shared.join("src/domain.num"),
            "module shared.domain\n\ntype SharedEvent {\n    message: Text\n}\n",
        )
        .unwrap();

        let err = load_program_input(&root).unwrap_err();

        assert!(err.contains("requires language 0.2.0"));
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(shared).unwrap();
    }

    fn init_git_repo(root: &Path) {
        run_git(["init", "--quiet"], root);
        run_git(["add", "num.toml", "src/domain.num"], root);
        let output = Command::new("git")
            .args([
                "-c",
                "user.email=num@example.com",
                "-c",
                "user.name=num",
                "commit",
                "--quiet",
                "-m",
                "init",
            ])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git commit failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn git_head_rev(root: &Path) -> String {
        let output = run_git(["rev-parse", "HEAD"], root);
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn run_git<const N: usize>(args: [&str; N], root: &Path) -> std::process::Output {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git command failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
}

#[cfg(test)]
fn path_to_toml_string(path: String) -> String {
    path.replace('\\', "\\\\").replace('"', "\\\"")
}
