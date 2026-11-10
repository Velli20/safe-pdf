//! Per-code metric runs: the ranges a font's `/W`, `/W2` and `/Widths` entries declare.

use pdf_cmap::CodeRange;

/// Metrics for the inclusive code range `first..=last`, `N` numbers per code.
#[derive(Debug, Clone)]
pub(crate) struct MetricRun<const N: usize> {
    first: u32,
    last: u32,
    values: RunValues<N>,
}

/// The metrics of one run.
#[derive(Debug, Clone)]
enum RunValues<const N: usize> {
    /// `c_first c_last v`: one value for every code in the run.
    Uniform([f32; N]),
    /// `c_first [v1 ... vn]` or a simple font's `/Widths`: one value per consecutive code.
    Explicit(Box<[[f32; N]]>),
}

impl<const N: usize> MetricRun<N> {
    /// One value shared by every code in `first..=last`.
    pub(crate) fn uniform(first: u32, last: u32, value: [f32; N]) -> Self {
        Self {
            first,
            last,
            values: RunValues::Uniform(value),
        }
    }

    /// Consecutive values starting at `first`.
    pub(crate) fn explicit(first: u32, values: Box<[[f32; N]]>) -> Self {
        let span = u32::try_from(values.len().saturating_sub(1)).unwrap_or(u32::MAX);
        Self {
            first,
            last: first.saturating_add(span),
            values: RunValues::Explicit(values),
        }
    }

    /// Returns the value `offset` codes past the start of the run.
    pub(crate) fn value(&self, offset: u32) -> Option<[f32; N]> {
        match &self.values {
            RunValues::Uniform(value) => Some(*value),
            RunValues::Explicit(values) => values.get(usize::try_from(offset).ok()?).copied(),
        }
    }
}

impl<const N: usize> CodeRange for MetricRun<N> {
    fn first(&self) -> u32 {
        self.first
    }

    fn last(&self) -> u32 {
        self.last
    }
}
