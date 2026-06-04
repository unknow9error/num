use crate::{package, registry};
use std::path::PathBuf;

pub fn run(args: impl Iterator<Item = String>) -> Result<(), String> {
    let mut args = args;
    match args.next().as_deref() {
        Some("publish") => publish(args),
        Some("list") => list(args),
        Some("install") => install(args),
        Some(other) => Err(format!(
            "unknown registry command `{other}`\n\nSupported registry commands:\n  publish\n  list\n  install"
        )),
        None => Err("usage: num registry <publish|list|install> [options]".to_string()),
    }
}

struct RegistryMutationOptions {
    registry_root: Option<PathBuf>,
    dry_run: bool,
    replace: bool,
    format_json: bool,
}

struct RegistryListOptions {
    registry_root: Option<PathBuf>,
    format_json: bool,
}

struct RegistryInstallOptions {
    package_name: String,
    package_version: String,
    install_root: PathBuf,
    registry_root: Option<PathBuf>,
    dry_run: bool,
    replace: bool,
    format_json: bool,
}

fn publish(args: impl Iterator<Item = String>) -> Result<(), String> {
    let (path, options) = parse_publish_args(args)?;
    let manifest = package::PackageManifest::discover(&path)?
        .ok_or_else(|| format!("no num.toml found for {}", path.display()))?;
    let registry = registry::registry_for_manifest(&manifest, options.registry_root.clone())?;
    let report = registry.publish(&manifest, options.dry_run, options.replace)?;
    if options.format_json {
        let json = serde_json::to_string_pretty(&report.to_json())
            .map_err(|err| format!("failed to render registry JSON: {err}"))?;
        println!("{json}");
    } else {
        print!("{}", report.render_text());
    }
    Ok(())
}

fn list(args: impl Iterator<Item = String>) -> Result<(), String> {
    let options = parse_list_args(args)?;
    let registry = registry::registry_from_arg(options.registry_root.clone())?;
    let report = registry.list()?;
    if options.format_json {
        let json = serde_json::to_string_pretty(&report.to_json())
            .map_err(|err| format!("failed to render registry JSON: {err}"))?;
        println!("{json}");
    } else {
        print!("{}", report.render_text());
    }
    Ok(())
}

fn install(args: impl Iterator<Item = String>) -> Result<(), String> {
    let options = parse_install_args(args)?;
    let registry = registry::registry_from_arg(options.registry_root.clone())?;
    let report = registry.install(
        &options.package_name,
        &options.package_version,
        &options.install_root,
        options.dry_run,
        options.replace,
    )?;
    if options.format_json {
        let json = serde_json::to_string_pretty(&report.to_json())
            .map_err(|err| format!("failed to render registry JSON: {err}"))?;
        println!("{json}");
    } else {
        print!("{}", report.render_text());
    }
    Ok(())
}

fn parse_publish_args(
    args: impl Iterator<Item = String>,
) -> Result<(PathBuf, RegistryMutationOptions), String> {
    let mut path = None;
    let mut options = RegistryMutationOptions {
        registry_root: None,
        dry_run: false,
        replace: false,
        format_json: false,
    };
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--registry" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "usage: --registry <registry-root>".to_string())?;
                options.registry_root = Some(PathBuf::from(raw));
            }
            "--dry-run" => options.dry_run = true,
            "--replace" => options.replace = true,
            "--json" => options.format_json = true,
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected registry publish argument '{arg}'")),
        }
    }

    Ok((path.unwrap_or_else(|| PathBuf::from(".")), options))
}

fn parse_list_args(args: impl Iterator<Item = String>) -> Result<RegistryListOptions, String> {
    let mut options = RegistryListOptions {
        registry_root: None,
        format_json: false,
    };
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--registry" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "usage: --registry <registry-root>".to_string())?;
                options.registry_root = Some(PathBuf::from(raw));
            }
            "--json" => options.format_json = true,
            _ => return Err(format!("unexpected registry list argument '{arg}'")),
        }
    }

    Ok(options)
}

fn parse_install_args(
    args: impl Iterator<Item = String>,
) -> Result<RegistryInstallOptions, String> {
    let mut package_name = None;
    let mut package_version = None;
    let mut install_root = PathBuf::from(".num/packages");
    let mut registry_root = None;
    let mut dry_run = false;
    let mut replace = false;
    let mut format_json = false;
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--registry" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "usage: --registry <registry-root>".to_string())?;
                registry_root = Some(PathBuf::from(raw));
            }
            "--to" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "usage: --to <install-root>".to_string())?;
                install_root = PathBuf::from(raw);
            }
            "--dry-run" => dry_run = true,
            "--replace" => replace = true,
            "--json" => format_json = true,
            _ if package_name.is_none() => package_name = Some(arg),
            _ if package_version.is_none() => package_version = Some(arg),
            _ => return Err(format!("unexpected registry install argument '{arg}'")),
        }
    }

    Ok(RegistryInstallOptions {
        package_name: package_name.ok_or_else(|| {
            "usage: num registry install <package-name> <version> [--to <install-root>]".to_string()
        })?,
        package_version: package_version.ok_or_else(|| {
            "usage: num registry install <package-name> <version> [--to <install-root>]".to_string()
        })?,
        install_root,
        registry_root,
        dry_run,
        replace,
        format_json,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_args_parse_path_and_flags() {
        let (path, options) = parse_publish_args(
            [
                "examples/refund_workflow".to_string(),
                "--registry".to_string(),
                "/tmp/num-registry".to_string(),
                "--replace".to_string(),
                "--dry-run".to_string(),
                "--json".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert_eq!(path, PathBuf::from("examples/refund_workflow"));
        assert_eq!(
            options.registry_root,
            Some(PathBuf::from("/tmp/num-registry"))
        );
        assert!(options.replace);
        assert!(options.dry_run);
        assert!(options.format_json);
    }

    #[test]
    fn list_args_parse_registry_and_json() {
        let options = parse_list_args(
            [
                "--registry".to_string(),
                "/tmp/num-registry".to_string(),
                "--json".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert_eq!(
            options.registry_root,
            Some(PathBuf::from("/tmp/num-registry"))
        );
        assert!(options.format_json);
    }

    #[test]
    fn install_args_parse_package_and_flags() {
        let options = parse_install_args(
            [
                "shared".to_string(),
                "1.2.3".to_string(),
                "--registry".to_string(),
                "/tmp/num-registry".to_string(),
                "--to".to_string(),
                "vendor".to_string(),
                "--replace".to_string(),
                "--json".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert_eq!(options.package_name, "shared");
        assert_eq!(options.package_version, "1.2.3");
        assert_eq!(
            options.registry_root,
            Some(PathBuf::from("/tmp/num-registry"))
        );
        assert_eq!(options.install_root, PathBuf::from("vendor"));
        assert!(options.replace);
        assert!(options.format_json);
    }
}
