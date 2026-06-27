use crate::project;
use num_compiler::{check_program, lexer, parser};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const DEFAULT_ITERATIONS: usize = 3;
const DEFAULT_MAX_PARSE_REGRESSION_PCT: f64 = 25.0;
const DEFAULT_MAX_CHECK_REGRESSION_PCT: f64 = 25.0;
const DEFAULT_MAX_PARSE_REGRESSION_NANOS: u128 = 5_000_000;
const DEFAULT_MAX_CHECK_REGRESSION_NANOS: u128 = 5_000_000;

#[derive(Debug, Clone)]
pub struct BenchOptions {
    pub root: PathBuf,
    pub iterations: usize,
    pub format_json: bool,
    pub compare: Option<BenchCompareOptions>,
}

#[derive(Debug, Clone)]
pub struct BenchCompareOptions {
    pub baseline: PathBuf,
    pub max_parse_regression_pct: f64,
    pub max_check_regression_pct: f64,
    pub max_parse_regression_nanos: u128,
    pub max_check_regression_nanos: u128,
}

#[derive(Debug, Clone)]
pub struct BenchReport {
    pub schema_version: u32,
    pub iterations: usize,
    pub fixtures_root: PathBuf,
    pub fixtures: Vec<FixtureReport>,
}

#[derive(Debug, Clone)]
pub struct BenchCompareReport {
    pub baseline: PathBuf,
    pub max_parse_regression_pct: f64,
    pub max_check_regression_pct: f64,
    pub max_parse_regression_nanos: u128,
    pub max_check_regression_nanos: u128,
    pub fixtures: Vec<FixtureComparison>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FixtureComparison {
    pub name: String,
    pub parse: TimingComparison,
    pub check: TimingComparison,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimingComparison {
    pub current_nanos: u128,
    pub baseline_nanos: Option<u128>,
    pub delta_nanos: Option<i128>,
    pub delta_pct: Option<f64>,
    pub threshold_pct: f64,
    pub threshold_nanos: u128,
    pub status: TimingComparisonStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingComparisonStatus {
    Passed,
    Regressed,
    MissingBaseline,
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
    let comparison = options
        .compare
        .as_ref()
        .map(|compare| compare_report(&report, compare))
        .transpose()?;
    if options.format_json {
        let mut payload = report.to_json();
        if let Some(comparison) = &comparison {
            payload["comparison"] = comparison.to_json();
        }
        let json = serde_json::to_string_pretty(&payload)
            .map_err(|err| format!("failed to render benchmark JSON: {err}"))?;
        println!("{json}");
    } else {
        print!("{}", report.render_text());
        if let Some(comparison) = &comparison {
            print!("{}", comparison.render_text());
        }
    }
    if let Some(comparison) = comparison {
        if comparison.has_regressions() {
            return Err("benchmark regression thresholds exceeded".to_string());
        }
    }
    Ok(())
}

pub fn parse_options(args: impl Iterator<Item = String>) -> Result<BenchOptions, String> {
    let mut root = None;
    let mut iterations = DEFAULT_ITERATIONS;
    let mut format_json = false;
    let mut compare_path = None;
    let mut max_parse_regression_pct = DEFAULT_MAX_PARSE_REGRESSION_PCT;
    let mut max_check_regression_pct = DEFAULT_MAX_CHECK_REGRESSION_PCT;
    let mut max_parse_regression_nanos = DEFAULT_MAX_PARSE_REGRESSION_NANOS;
    let mut max_check_regression_nanos = DEFAULT_MAX_CHECK_REGRESSION_NANOS;
    let mut pending_iterations = false;
    let mut pending_compare = false;
    let mut pending_parse_pct = false;
    let mut pending_check_pct = false;
    let mut pending_parse_nanos = false;
    let mut pending_check_nanos = false;

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
        if pending_compare {
            compare_path = Some(PathBuf::from(arg));
            pending_compare = false;
            continue;
        }
        if pending_parse_pct {
            max_parse_regression_pct = parse_non_negative_f64(&arg, "--max-parse-regression-pct")?;
            pending_parse_pct = false;
            continue;
        }
        if pending_check_pct {
            max_check_regression_pct = parse_non_negative_f64(&arg, "--max-check-regression-pct")?;
            pending_check_pct = false;
            continue;
        }
        if pending_parse_nanos {
            max_parse_regression_nanos =
                parse_non_negative_u128(&arg, "--max-parse-regression-nanos")?;
            pending_parse_nanos = false;
            continue;
        }
        if pending_check_nanos {
            max_check_regression_nanos =
                parse_non_negative_u128(&arg, "--max-check-regression-nanos")?;
            pending_check_nanos = false;
            continue;
        }

        match arg.as_str() {
            "--json" => format_json = true,
            "--iterations" => pending_iterations = true,
            "--compare" => pending_compare = true,
            "--max-parse-regression-pct" => pending_parse_pct = true,
            "--max-check-regression-pct" => pending_check_pct = true,
            "--max-parse-regression-nanos" => pending_parse_nanos = true,
            "--max-check-regression-nanos" => pending_check_nanos = true,
            other if other.starts_with("--iterations=") => {
                let value = other.trim_start_matches("--iterations=");
                iterations = value.parse::<usize>().map_err(|_| {
                    format!("invalid --iterations value '{value}'; expected a positive integer")
                })?;
                if iterations == 0 {
                    return Err("--iterations must be greater than 0".to_string());
                }
            }
            other if other.starts_with("--compare=") => {
                let value = other.trim_start_matches("--compare=");
                if value.is_empty() {
                    return Err("missing value for --compare".to_string());
                }
                compare_path = Some(PathBuf::from(value));
            }
            other if other.starts_with("--max-parse-regression-pct=") => {
                max_parse_regression_pct = parse_non_negative_f64(
                    other.trim_start_matches("--max-parse-regression-pct="),
                    "--max-parse-regression-pct",
                )?;
            }
            other if other.starts_with("--max-check-regression-pct=") => {
                max_check_regression_pct = parse_non_negative_f64(
                    other.trim_start_matches("--max-check-regression-pct="),
                    "--max-check-regression-pct",
                )?;
            }
            other if other.starts_with("--max-parse-regression-nanos=") => {
                max_parse_regression_nanos = parse_non_negative_u128(
                    other.trim_start_matches("--max-parse-regression-nanos="),
                    "--max-parse-regression-nanos",
                )?;
            }
            other if other.starts_with("--max-check-regression-nanos=") => {
                max_check_regression_nanos = parse_non_negative_u128(
                    other.trim_start_matches("--max-check-regression-nanos="),
                    "--max-check-regression-nanos",
                )?;
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
    if pending_compare {
        return Err("missing value for --compare".to_string());
    }
    if pending_parse_pct {
        return Err("missing value for --max-parse-regression-pct".to_string());
    }
    if pending_check_pct {
        return Err("missing value for --max-check-regression-pct".to_string());
    }
    if pending_parse_nanos {
        return Err("missing value for --max-parse-regression-nanos".to_string());
    }
    if pending_check_nanos {
        return Err("missing value for --max-check-regression-nanos".to_string());
    }

    Ok(BenchOptions {
        root: root.unwrap_or_else(default_fixture_root),
        iterations,
        format_json,
        compare: compare_path.map(|baseline| BenchCompareOptions {
            baseline,
            max_parse_regression_pct,
            max_check_regression_pct,
            max_parse_regression_nanos,
            max_check_regression_nanos,
        }),
    })
}

fn parse_non_negative_f64(value: &str, flag: &str) -> Result<f64, String> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| format!("invalid {flag} value '{value}'; expected a non-negative number"))?;
    if !parsed.is_finite() || parsed < 0.0 {
        return Err(format!(
            "invalid {flag} value '{value}'; expected a non-negative finite number"
        ));
    }
    Ok(parsed)
}

fn parse_non_negative_u128(value: &str, flag: &str) -> Result<u128, String> {
    value
        .parse::<u128>()
        .map_err(|_| format!("invalid {flag} value '{value}'; expected a non-negative integer"))
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

pub fn compare_report(
    report: &BenchReport,
    options: &BenchCompareOptions,
) -> Result<BenchCompareReport, String> {
    let baseline = read_baseline(&options.baseline)?;
    let fixtures = report
        .fixtures
        .iter()
        .map(|fixture| {
            let baseline = baseline.get(&fixture.name);
            FixtureComparison {
                name: fixture.name.clone(),
                parse: compare_timing(
                    fixture.parse_nanos,
                    baseline.map(|fixture| fixture.parse_nanos),
                    options.max_parse_regression_pct,
                    options.max_parse_regression_nanos,
                ),
                check: compare_timing(
                    fixture.check_nanos,
                    baseline.map(|fixture| fixture.check_nanos),
                    options.max_check_regression_pct,
                    options.max_check_regression_nanos,
                ),
            }
        })
        .collect();
    Ok(BenchCompareReport {
        baseline: options.baseline.clone(),
        max_parse_regression_pct: options.max_parse_regression_pct,
        max_check_regression_pct: options.max_check_regression_pct,
        max_parse_regression_nanos: options.max_parse_regression_nanos,
        max_check_regression_nanos: options.max_check_regression_nanos,
        fixtures,
    })
}

fn read_baseline(path: &Path) -> Result<HashMap<String, FixtureReport>, String> {
    let source = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let payload = serde_json::from_str::<Value>(&source).map_err(|err| {
        format!(
            "failed to parse benchmark baseline {}: {err}",
            path.display()
        )
    })?;
    if payload["schema_version"].as_u64() != Some(1) {
        return Err(format!(
            "benchmark baseline {} uses unsupported schema_version `{}`",
            path.display(),
            payload["schema_version"]
        ));
    }
    let fixtures = payload["fixtures"].as_array().ok_or_else(|| {
        format!(
            "benchmark baseline {} is missing array field `fixtures`",
            path.display()
        )
    })?;
    let mut out = HashMap::new();
    for fixture in fixtures {
        let report = FixtureReport::from_json(fixture)?;
        out.insert(report.name.clone(), report);
    }
    Ok(out)
}

fn compare_timing(
    current_nanos: u128,
    baseline_nanos: Option<u128>,
    threshold_pct: f64,
    threshold_nanos: u128,
) -> TimingComparison {
    let Some(baseline_nanos) = baseline_nanos else {
        return TimingComparison {
            current_nanos,
            baseline_nanos: None,
            delta_nanos: None,
            delta_pct: None,
            threshold_pct,
            threshold_nanos,
            status: TimingComparisonStatus::MissingBaseline,
        };
    };
    let delta_nanos = current_nanos as i128 - baseline_nanos as i128;
    let delta_pct = if baseline_nanos == 0 {
        if current_nanos == 0 {
            0.0
        } else {
            f64::INFINITY
        }
    } else {
        delta_nanos as f64 * 100.0 / baseline_nanos as f64
    };
    let status = if delta_nanos > threshold_nanos as i128 && delta_pct > threshold_pct {
        TimingComparisonStatus::Regressed
    } else {
        TimingComparisonStatus::Passed
    };

    TimingComparison {
        current_nanos,
        baseline_nanos: Some(baseline_nanos),
        delta_nanos: Some(delta_nanos),
        delta_pct: Some(delta_pct),
        threshold_pct,
        threshold_nanos,
        status,
    }
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

impl BenchCompareReport {
    pub fn has_regressions(&self) -> bool {
        self.fixtures.iter().any(FixtureComparison::has_regression)
    }

    pub fn to_json(&self) -> Value {
        json!({
            "baseline": self.baseline.display().to_string(),
            "status": if self.has_regressions() { "regressed" } else { "passed" },
            "thresholds": {
                "parse": {
                    "max_regression_pct": self.max_parse_regression_pct,
                    "max_regression_nanos": self.max_parse_regression_nanos,
                },
                "check": {
                    "max_regression_pct": self.max_check_regression_pct,
                    "max_regression_nanos": self.max_check_regression_nanos,
                },
            },
            "fixtures": self.fixtures.iter().map(FixtureComparison::to_json).collect::<Vec<_>>(),
        })
    }

    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!(
            "\ncomparison baseline: {}\n",
            self.baseline.display()
        ));
        output.push_str(
            "fixture                 phase  base_ms  current_ms  delta_ms  delta_pct  status\n",
        );
        for fixture in &self.fixtures {
            output.push_str(&fixture.render_timing_row("parse", &fixture.parse));
            output.push_str(&fixture.render_timing_row("check", &fixture.check));
        }
        output
    }
}

impl FixtureComparison {
    fn has_regression(&self) -> bool {
        self.parse.status == TimingComparisonStatus::Regressed
            || self.check.status == TimingComparisonStatus::Regressed
    }

    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "parse": self.parse.to_json(),
            "check": self.check.to_json(),
            "status": if self.has_regression() { "regressed" } else { "passed" },
        })
    }

    fn render_timing_row(&self, phase: &str, timing: &TimingComparison) -> String {
        let baseline = timing
            .baseline_nanos
            .map(|nanos| format!("{:.3}", nanos_to_ms(nanos)))
            .unwrap_or_else(|| "-".to_string());
        let delta = timing
            .delta_nanos
            .map(|nanos| format!("{:.3}", nanos as f64 / 1_000_000.0))
            .unwrap_or_else(|| "-".to_string());
        let pct = timing
            .delta_pct
            .map(|pct| format!("{pct:.1}%"))
            .unwrap_or_else(|| "-".to_string());
        format!(
            "{:<23} {:<5} {:>8} {:>10.3} {:>9} {:>10}  {}\n",
            truncate_name(&self.name, 23),
            phase,
            baseline,
            nanos_to_ms(timing.current_nanos),
            delta,
            pct,
            timing.status.as_str()
        )
    }
}

impl TimingComparison {
    fn to_json(&self) -> Value {
        json!({
            "current_nanos": self.current_nanos,
            "baseline_nanos": self.baseline_nanos,
            "delta_nanos": self.delta_nanos,
            "delta_pct": self.delta_pct,
            "threshold_pct": self.threshold_pct,
            "threshold_nanos": self.threshold_nanos,
            "status": self.status.as_str(),
        })
    }
}

impl TimingComparisonStatus {
    fn as_str(&self) -> &'static str {
        match self {
            TimingComparisonStatus::Passed => "passed",
            TimingComparisonStatus::Regressed => "regressed",
            TimingComparisonStatus::MissingBaseline => "missing_baseline",
        }
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

    fn from_json(value: &Value) -> Result<Self, String> {
        let name = required_string(value, "name")?;
        let path = required_string(value, "path")?;
        let timings = value
            .get("timings")
            .ok_or_else(|| format!("benchmark baseline fixture `{name}` is missing `timings`"))?;
        Ok(Self {
            name,
            path: PathBuf::from(path),
            source_files: optional_usize(value, "source_files")?,
            source_bytes: optional_usize(value, "source_bytes")?,
            source_lines: optional_usize(value, "source_lines")?,
            diagnostics: optional_usize(value, "diagnostics")?,
            lex_nanos: required_u128(timings, "lex_nanos")?,
            parse_nanos: required_u128(timings, "parse_nanos")?,
            check_nanos: required_u128(timings, "check_nanos")?,
        })
    }
}

fn required_string(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("benchmark baseline fixture is missing string field `{field}`"))
}

fn required_u128(value: &Value, field: &str) -> Result<u128, String> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .map(u128::from)
        .ok_or_else(|| format!("benchmark baseline timing is missing integer field `{field}`"))
}

fn optional_usize(value: &Value, field: &str) -> Result<usize, String> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .map(|value| usize::try_from(value).unwrap_or(usize::MAX))
        .ok_or_else(|| format!("benchmark baseline fixture is missing integer field `{field}`"))
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
    use std::sync::atomic::{AtomicU64, Ordering};

    static BASELINE_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn parse_options_defaults_to_checked_in_fixtures() {
        let options = parse_options(Vec::<String>::new().into_iter()).unwrap();

        assert_eq!(options.iterations, DEFAULT_ITERATIONS);
        assert!(!options.format_json);
        assert!(options.compare.is_none());
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
    fn parse_options_accepts_compare_thresholds() {
        let options = parse_options(
            [
                "--compare",
                "baseline.json",
                "--max-parse-regression-pct=10",
                "--max-check-regression-pct",
                "15",
                "--max-parse-regression-nanos=1000",
                "--max-check-regression-nanos",
                "2000",
            ]
            .into_iter()
            .map(String::from),
        )
        .unwrap();
        let compare = options.compare.unwrap();

        assert_eq!(compare.baseline, PathBuf::from("baseline.json"));
        assert_eq!(compare.max_parse_regression_pct, 10.0);
        assert_eq!(compare.max_check_regression_pct, 15.0);
        assert_eq!(compare.max_parse_regression_nanos, 1000);
        assert_eq!(compare.max_check_regression_nanos, 2000);
    }

    #[test]
    fn default_fixtures_produce_json_report() {
        let options = BenchOptions {
            root: default_fixture_root(),
            iterations: 1,
            format_json: true,
            compare: None,
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

    #[test]
    fn compare_report_marks_regression_only_past_percent_and_absolute_thresholds() {
        let baseline_path = write_baseline(&[("small", 100_000, 100_000)]);
        let report = BenchReport {
            schema_version: 1,
            iterations: 1,
            fixtures_root: PathBuf::from("fixtures"),
            fixtures: vec![fixture_report("small", 101_000, 130_001)],
        };
        let options = BenchCompareOptions {
            baseline: baseline_path.clone(),
            max_parse_regression_pct: 0.0,
            max_check_regression_pct: 10.0,
            max_parse_regression_nanos: 5_000,
            max_check_regression_nanos: 5_000,
        };

        let comparison = compare_report(&report, &options).unwrap();

        assert_eq!(
            comparison.fixtures[0].parse.status,
            TimingComparisonStatus::Passed
        );
        assert_eq!(
            comparison.fixtures[0].check.status,
            TimingComparisonStatus::Regressed
        );
        assert!(comparison.has_regressions());
        let _ = fs::remove_file(baseline_path);
    }

    #[test]
    fn compare_report_marks_missing_baseline_without_failing() {
        let baseline_path = write_baseline(&[("other", 100_000, 100_000)]);
        let report = BenchReport {
            schema_version: 1,
            iterations: 1,
            fixtures_root: PathBuf::from("fixtures"),
            fixtures: vec![fixture_report("small", 200_000, 200_000)],
        };
        let options = BenchCompareOptions {
            baseline: baseline_path.clone(),
            max_parse_regression_pct: 0.0,
            max_check_regression_pct: 0.0,
            max_parse_regression_nanos: 0,
            max_check_regression_nanos: 0,
        };

        let comparison = compare_report(&report, &options).unwrap();

        assert_eq!(
            comparison.fixtures[0].parse.status,
            TimingComparisonStatus::MissingBaseline
        );
        assert!(!comparison.has_regressions());
        let _ = fs::remove_file(baseline_path);
    }

    fn fixture_report(name: &str, parse_nanos: u128, check_nanos: u128) -> FixtureReport {
        FixtureReport {
            name: name.to_string(),
            path: PathBuf::from(name),
            source_files: 1,
            source_bytes: 10,
            source_lines: 1,
            diagnostics: 0,
            lex_nanos: 1,
            parse_nanos,
            check_nanos,
        }
    }

    fn write_baseline(fixtures: &[(&str, u128, u128)]) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "num-bench-baseline-{}-{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            BASELINE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let fixtures = fixtures
            .iter()
            .map(|(name, parse_nanos, check_nanos)| {
                fixture_report(name, *parse_nanos, *check_nanos).to_json()
            })
            .collect::<Vec<_>>();
        fs::write(
            &path,
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "iterations": 1,
                "fixtures_root": "fixtures",
                "fixtures": fixtures,
            }))
            .unwrap(),
        )
        .unwrap();
        path
    }
}
