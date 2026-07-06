use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("compatibility")
        .join(name)
}

fn temp_project(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let target = std::env::temp_dir().join(format!("num_compat_matrix_{name}_{stamp}"));
    copy_dir(&fixture(name), &target);
    target
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let source = entry.path();
        let target = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir(&source, &target);
        } else {
            fs::copy(&source, &target).unwrap();
        }
    }
}

fn run_num(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_num"))
        .args(args)
        .output()
        .unwrap()
}

fn path_arg(path: &Path) -> String {
    path.display().to_string()
}

fn stdout_json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn stdout_json_unchecked(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn current_manifest_is_compatible() {
    let project = fixture("current");
    let output = run_num(&["compat", &path_arg(&project), "--json"]);
    let report = stdout_json(&output);

    assert_eq!(report[0]["status"], "compatible");
    assert_eq!(report[0]["language"]["version"], "0.4.10");
    assert_eq!(report[0]["language"]["current"], "0.4.10");
    assert_eq!(report[0]["manifest"]["schema"], 1);
    assert_eq!(report[0]["manifest"]["current_schema"], 1);
}

#[test]
fn exact_current_manifest_is_compatible() {
    let project = fixture("exact_current");
    let output = run_num(&["compat", &path_arg(&project), "--json"]);
    let report = stdout_json(&output);

    assert_eq!(report[0]["status"], "compatible");
    assert_eq!(report[0]["language"]["compatibility"], "exact");
}

#[test]
fn missing_language_section_is_planned_as_migratable() {
    let project = fixture("legacy_missing_language");
    let output = run_num(&["migrate", &path_arg(&project), "--json"]);
    let report = stdout_json(&output);

    assert_eq!(report["changed"], true);
    assert_eq!(report["applied"], false);
    assert!(report["actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action == "insert [language] section with current language/schema metadata"));
}

#[test]
fn schema_zero_migrates_to_current_schema_and_then_passes_compat() {
    let project = temp_project("schema_0");
    let project_arg = path_arg(&project);

    let before = run_num(&["compat", &project_arg, "--json"]);
    assert!(!before.status.success());
    assert!(stderr(&before).contains("declares invalid [language].manifest_schema 0"));
    let before_report = stdout_json_unchecked(&before);
    assert_eq!(before_report[0]["status"], "incompatible");
    assert!(before_report[0]["reason"]
        .as_str()
        .unwrap()
        .contains("declares invalid [language].manifest_schema 0"));

    let migration = run_num(&["migrate", &project_arg, "--write", "--json"]);
    let migration_report = stdout_json(&migration);
    assert_eq!(migration_report["changed"], true);
    assert_eq!(migration_report["applied"], true);
    assert!(fs::read_to_string(project.join("num.toml"))
        .unwrap()
        .contains("manifest_schema = 1"));

    let after = run_num(&["compat", &project_arg, "--json"]);
    let compat_report = stdout_json(&after);
    assert_eq!(compat_report[0]["status"], "compatible");

    fs::remove_dir_all(project).unwrap();
}

#[test]
fn legacy_missing_module_source_migrates_and_then_checks() {
    let project = temp_project("legacy_missing_module");
    let project_arg = path_arg(&project);
    let main_path = project.join("src/main.num");

    let plan = run_num(&["migrate", &project_arg, "--source", "--json"]);
    let plan_report = stdout_json(&plan);
    assert_eq!(plan_report["changed"], true);
    assert_eq!(plan_report["applied"], false);
    assert!(plan_report["files"][0]["actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action == "insert explicit module declaration `module main`"));
    assert!(!fs::read_to_string(&main_path)
        .unwrap()
        .contains("module main"));

    let migration = run_num(&["migrate", &project_arg, "--source", "--write", "--json"]);
    let migration_report = stdout_json(&migration);
    assert_eq!(migration_report["changed"], true);
    assert_eq!(migration_report["applied"], true);
    let migrated_source = fs::read_to_string(&main_path).unwrap();
    assert!(migrated_source.contains("// Legacy source"));
    assert!(migrated_source.contains("module main\n\nworkflow main()"));

    let check = run_num(&["check", &project_arg]);
    assert!(
        check.status.success(),
        "check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    fs::remove_dir_all(project).unwrap();
}

#[test]
fn legacy_lock_missing_schema_migrates_and_then_checks() {
    let project = temp_project("legacy_lock_missing_schema");
    let project_arg = path_arg(&project);
    let lock_path = project.join("num.lock");

    let before = run_num(&["lock", &project_arg, "--check"]);
    assert!(!before.status.success());
    assert!(stderr(&before).contains("missing lockfile `version`"));

    let plan = run_num(&["lock", &project_arg, "--migrate", "--json"]);
    let plan_report = stdout_json(&plan);
    assert_eq!(plan_report["changed"], true);
    assert_eq!(plan_report["applied"], false);
    assert_eq!(plan_report["schema"], Value::Null);
    assert!(plan_report["actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action == "add lockfile schema version 1"));
    assert!(!fs::read_to_string(&lock_path)
        .unwrap()
        .contains("version = 1\n\n[[package]]"));

    let migration = run_num(&["lock", &project_arg, "--migrate", "--write", "--json"]);
    let migration_report = stdout_json(&migration);
    assert_eq!(migration_report["changed"], true);
    assert_eq!(migration_report["applied"], true);
    let migrated_lockfile = fs::read_to_string(&lock_path).unwrap();
    assert!(migrated_lockfile.contains("version = 1\n\n[[package]]"));
    assert!(migrated_lockfile.contains("legacy-lock-missing-schema"));

    let after = run_num(&["lock", &project_arg, "--check"]);
    assert!(
        after.status.success(),
        "lock check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&after.stdout),
        String::from_utf8_lossy(&after.stderr)
    );

    fs::remove_dir_all(project).unwrap();
}

#[test]
fn lock_schema_zero_migrates_and_then_checks() {
    let project = temp_project("lock_schema_0");
    let project_arg = path_arg(&project);
    let lock_path = project.join("num.lock");

    let before = run_num(&["lock", &project_arg, "--check"]);
    assert!(!before.status.success());
    assert!(stderr(&before).contains("invalid lockfile version 0"));

    let migration = run_num(&["lock", &project_arg, "--migrate", "--write", "--json"]);
    let migration_report = stdout_json(&migration);
    assert_eq!(migration_report["changed"], true);
    assert_eq!(migration_report["applied"], true);
    assert_eq!(migration_report["schema"], 0);
    assert!(migration_report["actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action == "upgrade lockfile schema from 0 to 1"));
    assert!(fs::read_to_string(&lock_path)
        .unwrap()
        .contains("version = 1\n\n[[package]]"));

    let after = run_num(&["lock", &project_arg, "--check"]);
    assert!(
        after.status.success(),
        "lock check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&after.stdout),
        String::from_utf8_lossy(&after.stderr)
    );

    fs::remove_dir_all(project).unwrap();
}

#[test]
fn future_schema_is_rejected_by_compatibility_and_migration() {
    let project = fixture("future_schema");
    let project_arg = path_arg(&project);

    let compat = run_num(&["compat", &project_arg, "--json"]);
    assert!(!compat.status.success());
    assert!(stderr(&compat).contains("requires manifest schema 2"));
    let compat_report = stdout_json_unchecked(&compat);
    assert_eq!(compat_report[0]["status"], "incompatible");
    assert!(compat_report[0]["reason"]
        .as_str()
        .unwrap()
        .contains("requires manifest schema 2"));

    let migration = run_num(&["migrate", &project_arg, "--json"]);
    assert!(!migration.status.success());
    assert!(stderr(&migration).contains("cannot migrate manifest schema 2"));
}

#[test]
fn future_language_is_rejected_and_not_downgraded_by_upgrade_version() {
    let project = fixture("future_language");
    let project_arg = path_arg(&project);

    let compat = run_num(&["compat", &project_arg, "--json"]);
    assert!(!compat.status.success());
    assert!(stderr(&compat).contains("requires language 0.5.0"));
    let compat_report = stdout_json_unchecked(&compat);
    assert_eq!(compat_report[0]["status"], "incompatible");
    assert!(compat_report[0]["reason"]
        .as_str()
        .unwrap()
        .contains("requires language 0.5.0"));

    let upgrade = run_num(&["upgrade-version", &project_arg, "--json"]);
    assert!(!upgrade.status.success());
    assert!(stderr(&upgrade).contains("refusing to downgrade [language].version"));
}

#[test]
fn upgrade_version_can_bump_project_version_without_breaking_compatibility() {
    let project = temp_project("current");
    let project_arg = path_arg(&project);

    let upgrade = run_num(&[
        "upgrade-version",
        &project_arg,
        "--project",
        "0.4.10",
        "--write",
        "--json",
    ]);
    let upgrade_report = stdout_json(&upgrade);
    assert_eq!(upgrade_report["applied"], true);
    assert_eq!(upgrade_report["project"]["from"], "0.1.0");
    assert_eq!(upgrade_report["project"]["to"], "0.4.10");

    let compat = run_num(&["compat", &project_arg, "--json"]);
    let compat_report = stdout_json(&compat);
    assert_eq!(compat_report[0]["status"], "compatible");
    assert_eq!(compat_report[0]["package"]["version"], "0.4.10");

    fs::remove_dir_all(project).unwrap();
}
