mod criterion;
mod plot;
mod trace;

use crate::trace::Traces;
use anyhow::{bail, Context};
use clap::Parser;
use regex::Regex;
use std::{num::NonZeroU32, path::Path};

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

    /// Width of the output image in pixels
    #[arg(short = 'W', long, default_value = "1920")]
    width: NonZeroU32,

    /// Height of the output image in pixels
    #[arg(short = 'H', long, default_value = "1080")]
    height: NonZeroU32,

    /// Title of the plot
    #[arg(short, long, default_value = "Benchmark results")]
    title: Box<str>,

    /// Forced lower bound of the Y axis
    ///
    /// Will automatically set the Y scale to fit all traces by default
    #[arg(short = 'y', long, default_value = None)]
    min_y: Option<f32>,

    /// Forced upper bound of the Y axis
    ///
    /// Will automatically set the Y scale to fit all traces by default
    #[arg(short = 'Y', long, default_value = None)]
    max_y: Option<f32>,

    /// Unit of element-based throughput measurement
    ///
    /// This will be used, along with an SI prefix and a "per second" suffix, to
    /// label the plot's vertical axis in the presence of such measurements.
    #[arg(short, long, default_value = "FLOP")]
    element_throughput_unit: Box<str>,

    /// Label of the horizontal axis
    ///
    /// Depending on the project, this can be an input size or an iteration
    /// count, so we need full control over labeling there.
    #[arg(short, long, default_value = "Input size (f32s)")]
    x_label: Box<str>,

    /// Regex matching the traces to be plotted
    regex: Regex,
}
//
impl Args {
    /// Plot size in plotters's expected format
    fn plot_size(&self) -> (u32, u32) {
        (self.width.get(), self.height.get())
    }
}
//
fn main() -> Result<()> {
    // Parse CLI arguments
    let args = Args::parse();

    // Load data points from Criterion
    let data = criterion::read_all(&args).context("loading data from Criterion")?;

    // Rearrange data in a layout suitable for plotting
    let traces = Traces::new(data).context("rearranging data into plot traces")?;

    // Abort if there is nothing to plot
    if traces.is_empty() {
        bail!("Specified regex does not select any trace!")
    }

    // Draw the plot
    plot::draw(&args, traces).context("drawing the performance plot")
}

/// Use anyhow for error handling convenience
pub use anyhow::Result;
