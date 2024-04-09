//! Benchmark traces suitable for plotting

use crate::{
    criterion::{self, Benchmark, BenchmarkInfo, Estimate, ThroughputType},
    Result,
};
use anyhow::ensure;
use std::{cmp::Ordering, collections::BTreeMap, iter::Peekable, ops::Range, str::CharIndices};

/// Set of traces to be plotted
#[derive(Clone, Debug, Default, PartialEq, PartialOrd)]
pub struct Traces {
    /// Throughput configuration, if any
    pub throughput: Option<ThroughputType>,

    /* /// Vertical axis multiple */
    /// Trace data
    pub per_trace_data: Box<[Trace]>,
}
//
impl Traces {
    /// Build traces from criterion benchmark data
    pub fn new(data: impl IntoIterator<Item = BenchmarkInfo>) -> Result<Self> {
        let mut name_to_trace = BTreeMap::<TraceName, BTreeMap<usize, MeasurementDisplay>>::new();
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

            let trace = name_to_trace.entry(TraceName(group_id)).or_default();
            ensure!(
                trace.insert(value, measurement).is_none(),
                "there should be only one data point associated with value {value}"
            );
        }
        let per_trace_data = name_to_trace
            .into_iter()
            .map(|(name, data)| Trace {
                name: name.0,
                data: data.into_iter().collect(),
            })
            .collect();
        Ok(Self {
            throughput: common_throughput_type,
            per_trace_data,
        })
    }

    /// Number of traces
    pub fn len(&self) -> usize {
        self.per_trace_data.len()
    }

    /// Absence of traces
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Horizontal and vertical range covered by traces
    pub fn xy_range(&self) -> (Range<f64>, Range<f32>) {
        let min_x = self
            .per_trace_data
            .iter()
            .map(|trace| trace.data.first().expect("traces can't be empty").0)
            .min()
            .expect("there should be >= 1 trace") as f64;
        let max_x = self
            .per_trace_data
            .iter()
            .map(|trace| trace.data.last().expect("traces can't be empty").0)
            .max()
            .expect("there should be >= 1 trace") as f64;
        let min_y = self
            .per_trace_data
            .iter()
            .flat_map(|trace| trace.data.iter())
            .map(|(_, meas)| meas.lower_bound)
            .min_by(f32::total_cmp)
            .expect("there should be >= 1 trace");
        let max_y = self
            .per_trace_data
            .iter()
            .flat_map(|trace| trace.data.iter())
            .map(|(_, meas)| meas.upper_bound)
            .max_by(f32::total_cmp)
            .expect("there should be >= 1 trace");
        (min_x..max_x, min_y..max_y)
    }
}

/// Trace name newtype with a more sensible ordering
#[derive(Clone, Debug, Eq, PartialEq)]
struct TraceName(Box<str>);
//
impl PartialOrd for TraceName {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
//
impl Ord for TraceName {
    fn cmp(&self, other: &Self) -> Ordering {
        let (mut segments1, mut segments2) = (self.0.split('/'), other.0.split('/'));
        loop {
            // Extract next pair of name segments to be compared, apply trivial
            // ordering if we're done with one of the trace names.
            let (segment1, segment2) = match (segments1.next(), segments2.next()) {
                (Some(s1), Some(s2)) => (s1, s2),
                (Some(_), None) => return Ordering::Greater,
                (None, Some(_)) => return Ordering::Less,
                (None, None) => return Ordering::Equal,
            };

            // Split each text segment into a stream of numbers and
            // non-numerical text
            let (mut fragments1, mut fragments2) =
                (TextAndNumbers::new(segment1), TextAndNumbers::new(segment2));
            loop {
                // Pick next pair of codepoints, handle trivial cases
                match (fragments1.next(), fragments2.next()) {
                    (Some(frag1), Some(frag2)) => match frag1.cmp(&frag2) {
                        Ordering::Less => return Ordering::Less,
                        Ordering::Equal => continue,
                        Ordering::Greater => return Ordering::Greater,
                    },
                    (Some(_), None) => return Ordering::Greater,
                    (None, Some(_)) => return Ordering::Less,
                    (None, None) => break,
                };
            }
        }
    }
}

/// Decompose a string into a sequence of decimal numbers and non-numerical text
#[derive(Debug)]
struct TextAndNumbers<'source> {
    /// Source string
    source: &'source str,

    /// Iterator over chars of `source` and associated indices
    char_indices: Peekable<CharIndices<'source>>,
}
//
impl<'source> TextAndNumbers<'source> {
    /// Start decomposing the source string
    pub fn new(source: &'source str) -> Self {
        Self {
            source,
            char_indices: source.char_indices().peekable(),
        }
    }
}
//
impl<'source> Iterator for TextAndNumbers<'source> {
    type Item = TextOrNumber<'source>;

    fn next(&mut self) -> Option<Self::Item> {
        let (start_idx, first_char) = self.char_indices.peek().copied()?;
        let parsing_number = first_char.is_ascii_digit();
        let (end_idx, _last_char) = std::iter::from_fn(|| {
            self.char_indices
                .next_if(|(_idx, c)| c.is_ascii_digit() == parsing_number)
        })
        .last()?;
        let selected = &self.source[start_idx..=end_idx];
        let result = if parsing_number {
            TextOrNumber::Number(selected.parse().expect("only picked base-10 digits"))
        } else {
            TextOrNumber::Text(selected)
        };
        Some(result)
    }
}

/// Either a decimal number or non-numerical text
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum TextOrNumber<'source> {
    /// Non-numerical text
    Text(&'source str),

    /// Decimal number
    Number(usize),
}

/// Trace to be plotted
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Trace {
    /// Name of the trace
    pub name: Box<str>,

    /// Data to be plotted
    pub data: Box<[(ProblemSize, MeasurementDisplay)]>,
}

/// Horizontal coordinate of a criterion benchmark
pub type ProblemSize = usize;

/// Summary of a criterion benchmark measurement for display
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub struct MeasurementDisplay {
    /// 95% lower bound
    pub lower_bound: f32,

    /// Central value
    pub point_estimate: f32,

    /// 95% upper bound
    pub upper_bound: f32,
}
//
impl MeasurementDisplay {
    /// Turn a timing measurement into a throughput measurement
    ///
    /// This function has two correctness preconditions:
    ///
    /// - The source measurement must be a timing measurement in nanoseconds
    ///   (e.g. the direct result of converting a criterion median Estimate)
    /// - For the final plot to make sense, all measurements must have the same
    ///   [`ThroughputType`].
    fn time_to_throughput(self, untyped_throughput: u64) -> Self {
        let untyped_throughput = untyped_throughput as f32;
        Self {
            point_estimate: untyped_throughput / (self.point_estimate * 1e-9),
            lower_bound: untyped_throughput / (self.upper_bound * 1e-9),
            upper_bound: untyped_throughput / (self.lower_bound * 1e-9),
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
