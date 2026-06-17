use crate::project;
use num_compiler::{check_program, lexer, parser};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const DEFAULT_ITERATIONS: usize = 3;

#[derive(Debug, Clone)]
pub struct BenchOptions {
    pub root: PathBuf,
    pub iterations: usize,
    pub format_json: bool,
}

#[derive(Debug, Clone)]
pub struct BenchReport {
    pub schema_version: u32,
    pub iterations: usize,
    pub fixtures_root: PathBuf,
    pub fixtures: Vec<FixtureReport>,
}

#[derive(Debug, Clone)]
pub struct FixtureReport {
    pub name: String,
    pub path: PathBuf,
    pub source_files: usize,
    pub source_bytes: usize,
    pub source_lines: usize,
    pub diagnostics: usize,
    pub lex_nanos: u128,
    pub parse_nanos: u128,
    pub check_nanos: u128,
}

pub fn run(args: impl Iterator<Item = String>) -> Result<(), String> {
    let options = parse_options(args)?;
    let report = run_bench(&options)?;
    if options.format_json {
        let json = serde_json::to_string_pretty(&report.to_json())
            .map_err(|err| format!("failed to render benchmark JSON: {err}"))?;
        println!("{json}");
    } else {
        print!("{}", report.render_text());
    }
    Ok(())
}

pub fn parse_options(args: impl Iterator<Item = String>) -> Result<BenchOptions, String> {
    let mut root = None;
    let mut iterations = DEFAULT_ITERATIONS;
    let mut format_json = false;
    let mut pending_iterations = false;

    for arg in args {
        if pending_iterations {
            iterations = arg.parse::<usize>().map_err(|_| {
                format!("invalid --iterations value '{arg}'; expected a positive integer")
            })?;
            if iterations == 0 {
                return Err("--iterations must be greater than 0".to_string());
            }
            pending_iterations = false;
            continue;
        }

        match arg.as_str() {
            "--json" => format_json = true,
            "--iterations" => pending_iterations = true,
            other if other.starts_with("--iterations=") => {
                let value = other.trim_start_matches("--iterations=");
                iterations = value.parse::<usize>().map_err(|_| {
                    format!("invalid --iterations value '{value}'; expected a positive integer")
                })?;
                if iterations == 0 {
                    return Err("--iterations must be greater than 0".to_string());
                }
            }
            other if other.starts_with("--") => {
                return Err(format!("unexpected bench argument '{other}'"));
            }
            _ if root.is_none() => root = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected bench argument '{arg}'")),
        }
    }

    if pending_iterations {
        return Err("missing value for --iterations".to_string());
    }

    Ok(BenchOptions {
        root: root.unwrap_or_else(default_fixture_root),
        iterations,
        format_json,
    })
}

pub fn run_bench(options: &BenchOptions) -> Result<BenchReport, String> {
    let fixtures = discover_fixtures(&options.root)?;
    let mut reports = Vec::new();

    for fixture in fixtures {
        let mut samples = Vec::new();
        for _ in 0..options.iterations {
            samples.push(measure_fixture(&fixture)?);
        }
        reports.push(summarize_fixture(&fixture, &samples));
    }

    Ok(BenchReport {
        schema_version: 1,
        iterations: options.iterations,
        fixtures_root: options.root.clone(),
        fixtures: reports,
    })
}

fn default_fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/benchmarks")
}

fn discover_fixtures(root: &Path) -> Result<Vec<PathBuf>, String> {
    if !root.exists() {
        return Err(format!(
            "benchmark fixture root not found: {}",
            root.display()
        ));
    }
    if root.is_file() {
        return Ok(vec![root.to_path_buf()]);
    }
    if root.join("num.toml").is_file() {
        return Ok(vec![root.to_path_buf()]);
    }

    let mut fixtures = Vec::new();
    for entry in
        fs::read_dir(root).map_err(|err| format!("failed to read {}: {err}", root.display()))?
    {
        let entry =
            entry.map_err(|err| format!("failed to read {} entry: {err}", root.display()))?;
        let path = entry.path();
        if path.is_dir() {
            fixtures.push(path);
        } else if path.extension().is_some_and(|extension| extension == "num") {
            fixtures.push(path);
        }
    }
    fixtures.sort();

    if fixtures.is_empty() {
        return Err(format!(
            "no benchmark fixtures found under {}",
            root.display()
        ));
    }
    Ok(fixtures)
}

#[derive(Debug, Clone)]
struct FixtureSample {
    source_files: usize,
    source_bytes: usize,
    source_lines: usize,
    diagnostics: usize,
    lex_time: Duration,
    parse_time: Duration,
    check_time: Duration,
}

fn measure_fixture(path: &Path) -> Result<FixtureSample, String> {
    let input = project::load_program_input(path)?;
    let source_files = input.files.len();
    let source_bytes = input.files.iter().map(|file| file.source.len()).sum();
    let source_lines = input
        .files
        .iter()
        .map(|file| file.source.lines().count())
        .sum();

    let lex_start = Instant::now();
    let lexed = input
        .files
        .iter()
        .map(|file| lexer::lex(&file.name, &file.source))
        .collect::<Vec<_>>();
    let lex_time = lex_start.elapsed();

    let parse_start = Instant::now();
    let _parsed = input
        .files
        .iter()
        .zip(lexed.iter())
        .map(|(file, lexed)| parser::parse(&file.name, &lexed.tokens))
        .collect::<Vec<_>>();
    let parse_time = parse_start.elapsed();

    let check_start = Instant::now();
    let checked = check_program(input.files);
    let check_time = check_start.elapsed();

    let diagnostics = checked.diagnostics.len();

    Ok(FixtureSample {
        source_files,
        source_bytes,
        source_lines,
        diagnostics,
        lex_time,
        parse_time,
        check_time,
    })
}

fn summarize_fixture(path: &Path, samples: &[FixtureSample]) -> FixtureReport {
    let first = samples.first().expect("benchmark samples are non-empty");
    FixtureReport {
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string()),
        path: path.to_path_buf(),
        source_files: first.source_files,
        source_bytes: first.source_bytes,
        source_lines: first.source_lines,
        diagnostics: first.diagnostics,
        lex_nanos: median_nanos(samples.iter().map(|sample| sample.lex_time)),
        parse_nanos: median_nanos(samples.iter().map(|sample| sample.parse_time)),
        check_nanos: median_nanos(samples.iter().map(|sample| sample.check_time)),
    }
}

fn median_nanos(times: impl Iterator<Item = Duration>) -> u128 {
    let mut nanos = times.map(|time| time.as_nanos()).collect::<Vec<_>>();
    nanos.sort_unstable();
    nanos[nanos.len() / 2]
}

impl BenchReport {
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "iterations": self.iterations,
            "fixtures_root": self.fixtures_root.display().to_string(),
            "fixtures": self.fixtures.iter().map(FixtureReport::to_json).collect::<Vec<_>>(),
        })
    }

    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!(
            "num bench: {} fixture(s), {} iteration(s)\n",
            self.fixtures.len(),
            self.iterations
        ));
        output.push_str(
            "fixture                 files  bytes   lines  diag  lex_ms  parse_ms  check_ms\n",
        );
        for fixture in &self.fixtures {
            output.push_str(&format!(
                "{:<23} {:>5} {:>6} {:>7} {:>5} {:>7.3} {:>9.3} {:>9.3}\n",
                truncate_name(&fixture.name, 23),
                fixture.source_files,
                fixture.source_bytes,
                fixture.source_lines,
                fixture.diagnostics,
                nanos_to_ms(fixture.lex_nanos),
                nanos_to_ms(fixture.parse_nanos),
                nanos_to_ms(fixture.check_nanos)
            ));
        }
        output
    }
}

impl FixtureReport {
    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "path": self.path.display().to_string(),
            "source_files": self.source_files,
            "source_bytes": self.source_bytes,
            "source_lines": self.source_lines,
            "diagnostics": self.diagnostics,
            "timings": {
                "lex_nanos": self.lex_nanos,
                "parse_nanos": self.parse_nanos,
                "check_nanos": self.check_nanos,
            },
        })
    }
}

fn nanos_to_ms(nanos: u128) -> f64 {
    nanos as f64 / 1_000_000.0
}

fn truncate_name(name: &str, max_chars: usize) -> String {
    if name.chars().count() <= max_chars {
        return name.to_string();
    }
    let mut truncated = name
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_options_defaults_to_checked_in_fixtures() {
        let options = parse_options(Vec::<String>::new().into_iter()).unwrap();

        assert_eq!(options.iterations, DEFAULT_ITERATIONS);
        assert!(!options.format_json);
        assert!(options.root.ends_with("tests/fixtures/benchmarks"));
    }

    #[test]
    fn parse_options_accepts_json_iterations_and_path() {
        let options = parse_options(
            [
                "--json",
                "--iterations",
                "5",
                "tests/fixtures/benchmarks/small",
            ]
            .into_iter()
            .map(String::from),
        )
        .unwrap();

        assert!(options.format_json);
        assert_eq!(options.iterations, 5);
        assert_eq!(
            options.root,
            PathBuf::from("tests/fixtures/benchmarks/small")
        );
    }

    #[test]
    fn default_fixtures_produce_json_report() {
        let options = BenchOptions {
            root: default_fixture_root(),
            iterations: 1,
            format_json: true,
        };

        let report = run_bench(&options).unwrap();
        let payload = report.to_json();

        assert_eq!(payload["schema_version"], 1);
        assert_eq!(payload["iterations"], 1);
        assert!(payload["fixtures"].as_array().unwrap().len() >= 3);
        assert!(payload["fixtures"][0]["timings"]["lex_nanos"].is_u64());
    }

    #[test]
    fn fixture_project_path_is_treated_as_one_fixture() {
        let root = default_fixture_root().join("medium");
        let fixtures = discover_fixtures(&root).unwrap();

        assert_eq!(fixtures, vec![root]);
    }
}
