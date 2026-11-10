//! Vertical glyph metrics decoded from a CID font's `/DW2` and `/W2` entries.

use crate::{cid_metric_runs::CidMetricRuns, metric_run::MetricRun};
use pdf_cmap::{CodeRanges, code_range};
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

/// `/DW2` value used when a CID font omits it: `[vy w1y]`.
const DEFAULT_DW2: [f32; 2] = [880.0, -1000.0];

/// Vertical metrics of one CID, in thousandths of text space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VerticalMetric {
    /// Vertical displacement of the glyph (`w1y`), normally negative.
    pub w1y: f32,
    /// Horizontal component of the position vector from horizontal to vertical origin.
    pub vx: f32,
    /// Vertical component of the position vector from horizontal to vertical origin.
    pub vy: f32,
}

/// Vertical metrics a CID font declares for vertical writing.
///
/// `/DW2 [vy w1y]` supplies the default position vector height and displacement; `/W2` lists
/// per-CID overrides as `c [w1y v1x v1y ...]` or `c_first c_last w1y v1x v1y`. Overrides are kept
/// as the ranges the font declares and never expanded per CID.
#[derive(Debug, Clone)]
pub struct CidVerticalMetrics {
    /// Default `[vy w1y]` from `/DW2`.
    default: [f32; 2],
    /// `/W2` runs, sorted by first CID and disjoint.
    runs: Vec<MetricRun<3>>,
}

impl Default for CidVerticalMetrics {
    fn default() -> Self {
        Self {
            default: DEFAULT_DW2,
            runs: Vec::new(),
        }
    }
}

impl CidVerticalMetrics {
    /// Returns the vertical metrics of `cid`, whose horizontal width is `w0`.
    ///
    /// A CID without a `/W2` entry uses `/DW2` and a position vector centred on its width.
    #[must_use]
    pub fn get(&self, cid: u32, w0: f32) -> VerticalMetric {
        self.runs
            .covering(cid)
            .and_then(|(run, offset)| run.value(offset))
            .map_or_else(
                || {
                    let [vy, w1y] = self.default;
                    VerticalMetric {
                        w1y,
                        vx: w0 / 2.0,
                        vy,
                    }
                },
                |[w1y, vx, vy]| VerticalMetric { w1y, vx, vy },
            )
    }
}

impl FromPdfObject for CidVerticalMetrics {
    /// Reads `/DW2` and `/W2` from a CID font dictionary.
    ///
    /// Both entries only matter for vertical writing, so a malformed value falls back to the
    /// defaults instead of discarding a font that may only be used horizontally.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        Ok(Self {
            default: context.lenient(b"DW2").unwrap_or(DEFAULT_DW2),
            runs: context
                .lenient::<CidMetricRuns<3>>(b"W2")
                .map(|CidMetricRuns(runs)| code_range::disjoint(runs))
                .unwrap_or_default(),
        })
    }
}
