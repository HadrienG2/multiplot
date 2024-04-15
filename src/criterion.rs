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

    /// Value of the benchmark within the group
    pub value_str: Box<str>,

    /// Throughput configuration
    //
    // TODO: Handle non-throughput (pure timing) measurements
    pub throughput: Throughput,
}
//
impl Benchmark {
    /// Decode the benchmark value as an integer
    ///
    /// Criterion allows any string in here, but I always use this field to
    /// record the input size or iteration count, and Plotters needs it to be a
    /// number for axis construction anyway...
    pub fn value_usize(&self) -> Result<usize> {
        self.value_str
            .parse()
            .context("expected a usize criterion benchmark ID, got something else")
    }
}

/// We reuse criterion's Throughput type, which is fine as long as it does not
/// change too often...
pub use criterion::Throughput;

/// [`Throughput`] type information, without a value
#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ThroughputType {
    /// Measure throughput in terms of bytes/second. The value should be the
    /// number of bytes processed by one iteration of the benchmarked code.
    /// Typically, this would be the length of an input string or &[u8].
    Bytes,

    /// Equivalent to Bytes, but the value will be reported in terms of
    /// kilobytes (1000 bytes) per second instead of kibibytes (1024 bytes) per
    /// second, megabytes instead of mibibytes, and gigabytes instead of
    /// gibibytes.
    BytesDecimal,

    /// Measure throughput in terms of elements/second. The value should be the
    /// number of elements processed by one iteration of the benchmarked code.
    /// Typically, this would be the size of a collection, but could also be the
    /// number of lines of input text or the number of values to parse.
    Elements,
}

/// Split the throughput type information from the inner value
pub fn split_throughput(throughput: Throughput) -> (ThroughputType, u64) {
    match throughput {
        Throughput::Bytes(b) => (ThroughputType::Bytes, b),
        Throughput::BytesDecimal(d) => (ThroughputType::BytesDecimal, d),
        Throughput::Elements(e) => (ThroughputType::Elements, e),
    }
}

/// Criterion estimates
#[derive(Debug, Deserialize)]
#[non_exhaustive]
pub struct Estimates {
    /// Median execution time (ns)
    pub median: Estimate,
}

/// Single criterion estimate
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
