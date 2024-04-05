//! Raw data from Criterion

use crate::{Args, Result};
use anyhow::{bail, ensure, Context};
use serde::Deserialize;
use std::{
    collections::HashMap,
    path::{Component, Path},
};
use walkdir::{DirEntry, WalkDir};

/// Read raw data from Criterion
pub fn read_all(args: &Args) -> Result<Vec<BenchmarkInfo>> {
    // Compute criterion data path, make sure it exists
    let criterion_path = args.input_path.join("target/criterion");
    ensure!(
        criterion_path.exists(),
        "No criterion data found. Have you run the benchmark yet?"
    );

    // Collect data from each benchmark group in one place
    let mut benchmarks = HashMap::<_, BenchmarkInfoBuilder>::new();

    // Walk the criterion path, looking for data
    for entry in WalkDir::new(&criterion_path)
        .into_iter()
        .filter_entry(dir_entry_filter(args, &criterion_path))
        .filter(|entry| entry.as_ref().map_or(true, |e| e.depth() >= 4))
    {
        // Check entry validity
        let entry = entry?;
        ensure!(
            entry.file_type().is_file(),
            "Should only walk through criterion data files"
        );

        // Load the JSON data
        let json_bytes = std::fs::read(entry.path()).context("Failed to read data file")?;

        // Access the record for this benchmark
        let relative_path = strip_base_path(&entry, &criterion_path);
        let parent_dir = relative_path
            .parent()
            .context("Data files should have a parent directory")?;
        let benchmark_info = benchmarks.entry(parent_dir.to_path_buf()).or_default();

        // Decode the JSON data
        let file_stem = relative_path
            .file_stem()
            .context("Should be a data file name")?
            .to_str()
            .context("Data file names should be valid Unicode")?;
        match file_stem {
            "benchmark" => {
                let benchmark = serde_json::from_slice::<Benchmark>(&json_bytes[..])
                    .context("Failed to decode criterion benchmark metadata")?;
                ensure!(
                    args.regex.is_match(&benchmark.group_id),
                    "Benchmark group ID should match user-specified regex if directory name does"
                );
                benchmark_info.benchmark = Some(benchmark);
            }
            "estimates" => {
                let estimates = serde_json::from_slice::<Estimates>(&json_bytes[..])
                    .context("Failed to decode criterion benchmark result estimates")?;
                ensure!(
                    estimates.median.confidence_interval.confidence_level == 0.95,
                    "Expecting standard 95% confidence intervals from Criterion"
                );
                benchmark_info.estimates = Some(estimates);
            }
            _ => bail!("No support for parsing this Criterion output yet"),
        }
    }

    // Validate final data consistency
    let mut result = Vec::with_capacity(benchmarks.len());
    for (path, info) in benchmarks {
        let BenchmarkInfoBuilder {
            benchmark: Some(benchmark),
            estimates: Some(estimates),
        } = info
        else {
            bail!("Did not get all expected data for one benchmark")
        };
        assert_eq!(
            guess_benchmark_name(
                path.components()
                    .next()
                    .expect("Should have a benchmark directory")
            ),
            &*benchmark.group_id,
            "Benchmark group directories do not follow expected naming convention"
        );
        result.push(BenchmarkInfo {
            benchmark,
            estimates,
        })
    }
    Ok(result)
}

/// What we should eventually know about a single Criterion benchmark
#[derive(Debug)]
#[non_exhaustive]
pub struct BenchmarkInfo {
    /// Criterion benchmark metadata
    pub benchmark: Benchmark,

    /// Benchmark result estimates
    pub estimates: Estimates,
}

/// What we know about a single Criterion benchmark during file parsing
#[derive(Debug, Default)]
#[non_exhaustive]
struct BenchmarkInfoBuilder {
    /// Criterion benchmark metadata
    benchmark: Option<Benchmark>,

    /// Benchmark result estimates
    estimates: Option<Estimates>,
}

/// Criterion benchmark metadata
#[derive(Debug, Deserialize)]
#[non_exhaustive]
pub struct Benchmark {
    /// Name of the benchmark group
    pub group_id: Box<str>,

    /// Throughput configuration
    pub throughput: Throughput,
}

/// We reuse criterion's Throughput type, which is fine as long as it does not
/// change too often...
pub use criterion::Throughput;

/// Criterion estimates
#[derive(Debug, Deserialize)]
#[non_exhaustive]
pub struct Estimates {
    /// Median execution time (ns)
    pub median: Estimate,
}

/// Single criterion estimage
#[derive(Debug, Deserialize)]
pub struct Estimate {
    /// Confidence interval
    pub confidence_interval: ConfidenceInterval,

    /// Point estimate
    pub point_estimate: f32,

    /// Standard error
    pub standard_error: f32,
}

/// Criterion confidence interval
#[derive(Debug, Deserialize)]
pub struct ConfidenceInterval {
    /// Level of confidence
    pub confidence_level: f32,

    /// Lower bound
    pub lower_bound: f32,

    /// Upper bound
    pub upper_bound: f32,
}

/// DirEntry filter that only picks benchmark output and parents thereof
fn dir_entry_filter<'res>(
    args: &'res Args,
    criterion_path: &'res Path,
) -> impl FnMut(&DirEntry) -> bool + 'res {
    move |entry| {
        // Discard the part of the path that we already know
        let relative_path = strip_base_path(entry, criterion_path);

        // Check benchmark group directory, reject HTML report
        let mut relative_components = relative_path.components();
        let Some(benchmark_group_dir) = relative_components.next() else {
            return true;
        };
        if benchmark_group_dir.as_os_str() == "report" {
            return false;
        }

        // Reverse-engineer group name from directory name
        let benchmark_group_name = guess_benchmark_name(benchmark_group_dir);

        // Check if group name matches user-specified regex
        if !args.regex.is_match(&benchmark_group_name) {
            return false;
        }

        // Check input size / iteration count directory, reject HTML report
        let Some(input_size_dir) = relative_components.next() else {
            return true;
        };
        if input_size_dir.as_os_str() == "report" {
            return false;
        }

        // Only accept the newest dataset
        let Some(data_dir) = relative_components.next() else {
            return true;
        };
        if data_dir.as_os_str() != "new" {
            return false;
        }

        // Only accept data files which we will actually use
        let Some(data_file) = relative_components.next() else {
            return true;
        };
        let data_file_str = data_file
            .as_os_str()
            .to_str()
            .expect("Criterion data files should have Unicode names");
        let data_file_wo_ext = data_file_str
            .strip_suffix(".json")
            .expect("Criterion data files should all be JSON");
        data_file_wo_ext == "benchmark" || data_file_wo_ext == "estimates"
    }
}

/// Strip base path from walked directory entry
fn strip_base_path<'entry>(entry: &'entry DirEntry, criterion_path: &Path) -> &'entry Path {
    entry
        .path()
        .strip_prefix(criterion_path)
        .expect("Entry paths should feature the full prefix")
}

/// Guess the benchmark group name from the first path component
fn guess_benchmark_name(benchmark_group_dir: Component<'_>) -> String {
    let benchmark_group_dir_name = benchmark_group_dir
        .as_os_str()
        .to_str()
        .expect("Benchmark directory names should be valid Unicode");
    benchmark_group_dir_name
        .chars()
        .map(|c| if c == '_' { '/' } else { c })
        .collect()
}