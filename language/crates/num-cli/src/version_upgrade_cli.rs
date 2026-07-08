use crate::version_upgrade::{self, VersionUpgradeOptions};
use std::path::PathBuf;

pub fn run(args: impl Iterator<Item = String>) -> Result<(), String> {
    let (path, options, format_json) = parse_args(args)?;
    if options.include_dependencies {
        let report = version_upgrade::upgrade_manifest_graph_versions(&path, &options)?;
        if format_json {
            let json = serde_json::to_string_pretty(&report.to_json())
                .map_err(|err| format!("failed to render version upgrade JSON: {err}"))?;
            println!("{json}");
        } else {
            print!("{}", report.render_text());
        }
        return Ok(());
    }

    let report = version_upgrade::upgrade_manifest_versions(&path, &options)?;
    if format_json {
        let json = serde_json::to_string_pretty(&report.to_json())
            .map_err(|err| format!("failed to render version upgrade JSON: {err}"))?;
        println!("{json}");
    } else {
        print!("{}", report.render_text());
    }
    Ok(())
}

fn parse_args(
    args: impl Iterator<Item = String>,
) -> Result<(PathBuf, VersionUpgradeOptions, bool), String> {
    let mut path = None;
    let mut options = VersionUpgradeOptions::default();
    let mut format_json = false;
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--language" => {
                options.target_language_version = args
                    .next()
                    .ok_or_else(|| "usage: --language <x.y.z>".to_string())?;
            }
            "--project" => {
                options.target_project_version = Some(
                    args.next()
                        .ok_or_else(|| "usage: --project <x.y.z>".to_string())?,
                );
            }
            "--write" => options.write = true,
            "--include-dependencies" => options.include_dependencies = true,
            "--write-dependencies" => {
                options.include_dependencies = true;
                options.write_dependencies = true;
            }
            "--json" => format_json = true,
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected upgrade-version argument '{arg}'")),
        }
    }

    Ok((
        path.unwrap_or_else(|| PathBuf::from(".")),
        options,
        format_json,
    ))
}

#[cfg(test)]
mod tests {
    use super::parse_args;
    use crate::compatibility::CURRENT_LANGUAGE_VERSION;
    use std::path::PathBuf;

    #[test]
    fn parses_upgrade_version_args() {
        let (path, options, format_json) = parse_args(
            [
                "examples/refund_workflow".to_string(),
                "--language".to_string(),
                "0.4.25".to_string(),
                "--project".to_string(),
                "1.0.0".to_string(),
                "--write".to_string(),
                "--json".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert_eq!(path, PathBuf::from("examples/refund_workflow"));
        assert_eq!(options.target_language_version, "0.4.25");
        assert_eq!(options.target_project_version, Some("1.0.0".to_string()));
        assert!(options.write);
        assert!(format_json);
    }

    #[test]
    fn parses_dependency_upgrade_flags() {
        let (_path, options, _format_json) = parse_args(
            [
                "--include-dependencies".to_string(),
                "--write-dependencies".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert!(options.include_dependencies);
        assert!(options.write_dependencies);
    }

    #[test]
    fn default_language_target_is_current_cli_version() {
        let (_path, options, _format_json) = parse_args([].into_iter()).unwrap();

        assert_eq!(options.target_language_version, CURRENT_LANGUAGE_VERSION);
    }
}
