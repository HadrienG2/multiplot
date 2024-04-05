mod criterion;

use anyhow::Context;
use clap::Parser;
use regex::Regex;
use std::path::Path;

/// Use anyhow for error handling convenience
pub use anyhow::Result;

/// Simple bulk plotter from criterion data
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Path to root of Rust project where criterion data was acquired
    #[arg(short, long, default_value = ".")]
    input_path: Box<Path>,

    /// Name of output image
    #[arg(short, long, default_value = "./output.svg")]
    output_path: Box<Path>,

    /// Base label for element throughput
    #[arg(short, long, default_value = "FLOP/s")]
    throughput_name: Box<str>,

    /// Regex matching the traces to be plotted
    regex: Regex,
}

fn main() -> Result<()> {
    // Parse CLI arguments
    let args = Args::parse();

    // Load data points from Criterion
    let data = criterion::read_all(&args).context("Loading data from Criterion")?;

    // TODO: Shuffle data into a form suitable for plotting (BTreeMap from trace
    //       name to BTreeMap from throughput to median estimate -> eventually
    //       sorted list of traces with sorted list of (throughput, median
    //       estimates) points for each)
    // TODO: Check that all points associated with each trace are associated
    //       with the same throughput type along the way.

    // TODO: Pick one color per trace from regularly spaced points on
    //       colorous::WARM or colorous::COOL

    // TODO: Draw the plot, with error bars
    // see
    // https://github.com/plotters-rs/plotters/blob/master/plotters/examples/errorbar.rs
    // TODO: Add axis
    dbg!(data);

    Ok(())
}
