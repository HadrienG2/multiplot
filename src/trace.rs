//! Benchmark traces suitable for plotting

use crate::{
    criterion::{self, Benchmark, BenchmarkInfo, Estimate, ThroughputType},
    Result,
};
use anyhow::ensure;
use std::collections::BTreeMap;

/// Set of traces to be plotted
#[derive(Clone, Debug, Default, PartialEq, PartialOrd)]
pub struct Traces {
    /// Throughput configuration, if any
    throughput_type: Option<ThroughputType>,

    /// Trace data
    traces: Box<[Trace]>,
}
//
impl Traces {
    /// Build traces from criterion benchmark data
    pub fn new(data: impl IntoIterator<Item = BenchmarkInfo>) -> Result<Self> {
        let mut name_to_trace = BTreeMap::<Box<str>, BTreeMap<usize, MeasurementDisplay>>::new();
        let mut common_throughput_type = None;
        for benchmark_info in data {
            let BenchmarkInfo {
                benchmark,
                estimates,
            } = benchmark_info;
            let value = benchmark.value_usize()?;
            let Benchmark {
                group_id,
                value_str: _,
                throughput,
            } = benchmark;
            let (throughput_type, untyped_throughput) = criterion::split_throughput(throughput);
            if let Some(common_type) = &mut common_throughput_type {
                ensure!(
                throughput_type == *common_type,
                "expected all traces to use throughput type {common_type:?}, but found {throughput_type:?}",
            );
            } else {
                common_throughput_type = Some(throughput_type);
            }
            let measurement = MeasurementDisplay::try_from(estimates.median)?
                .time_to_throughput(untyped_throughput);

            let trace = name_to_trace.entry(group_id).or_default();
            ensure!(
                trace.insert(value, measurement).is_none(),
                "there should be only one data point associated with value {value}"
            );
        }
        let traces = name_to_trace
            .into_iter()
            .map(|(name, data)| Trace {
                name,
                data: data.into_iter().collect(),
            })
            .collect();
        Ok(Self {
            throughput_type: common_throughput_type,
            traces,
        })
    }
}

/// Trace to be plotted
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Trace {
    /// Name of the trace
    name: Box<str>,

    /// Data to be plotted
    data: Box<[(ProblemSize, MeasurementDisplay)]>,
}

/// Horizontal coordinate of a criterion benchmark
pub type ProblemSize = usize;

/// Summary of a criterion benchmark measurement for display
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub struct MeasurementDisplay {
    /// Central value
    point_estimate: f32,

    /// 95% lower bound
    lower_bound: f32,

    /// 95% upper bound
    upper_bound: f32,
}
//
impl MeasurementDisplay {
    /// Turn a timing measurement into a throughput measurement
    ///
    /// This function has two correctness preconditions:
    ///
    /// - The source measurement must be a timing measurement (e.g. the direct
    ///   result of converting a criterion median Estimate)
    /// - For the final plot to make sense, all measurements must have the same
    ///   [`ThroughputType`].
    fn time_to_throughput(self, untyped_throughput: u64) -> Self {
        let untyped_throughput = untyped_throughput as f32;
        Self {
            point_estimate: untyped_throughput / self.point_estimate,
            lower_bound: untyped_throughput / self.upper_bound,
            upper_bound: untyped_throughput / self.lower_bound,
        }
    }
}
//
impl TryFrom<Estimate> for MeasurementDisplay {
    type Error = anyhow::Error;

    fn try_from(value: Estimate) -> Result<Self> {
        ensure!(
            value.confidence_interval.confidence_level == 0.95,
            "Expecting standard 95% confidence intervals from Criterion"
        );
        Ok(Self {
            point_estimate: value.point_estimate,
            lower_bound: value.confidence_interval.lower_bound,
            upper_bound: value.confidence_interval.upper_bound,
        })
    }
}
