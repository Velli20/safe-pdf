//! Vertical glyph metrics decoded from a CID font's `/DW2` and `/W2` entries.

use crate::glyph_widths_map::GlyphWidthsMapError;
use pdf_cmap::{CodeRange, CodeRanges, code_range};
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, object_variant::ObjectVariant,
};

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
    runs: Vec<VerticalRun>,
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
            .and_then(|(run, offset)| run.metric(offset))
            .unwrap_or_else(|| {
                let [vy, w1y] = self.default;
                VerticalMetric {
                    w1y,
                    vx: w0 / 2.0,
                    vy,
                }
            })
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
                .lenient::<VerticalRuns>(b"W2")
                .map(|VerticalRuns(runs)| code_range::disjoint(runs))
                .unwrap_or_default(),
        })
    }
}

/// The runs of a `/W2` array, omitting empty ones.
struct VerticalRuns(Vec<VerticalRun>);

/// Reads each `c [w1y v1x v1y ...]` or `c_first c_last w1y v1x v1y` entry of a `/W2` array.
impl FromPdfObject for VerticalRuns {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut items = context.array()?;
        let mut entries = items.array().iter();
        let mut runs = Vec::new();
        while let Some(first) = entries.next() {
            let first = items.read::<u32>(first)?;
            let second = entries
                .next()
                .ok_or(GlyphWidthsMapError::IncompleteCIDEntry { cid: first })?;
            // The second item selects the form; an indirect metrics array is allowed.
            let second = items.resolve(second)?;
            if let ObjectVariant::Array(_) = second.value() {
                let values = items.read::<Vec<f32>>(second.value())?;
                let (triples, _) = values.as_chunks::<3>();
                if !triples.is_empty() {
                    let metrics = triples
                        .iter()
                        .map(|&[w1y, vx, vy]| VerticalMetric { w1y, vx, vy })
                        .collect::<Box<[_]>>();
                    let span = u32::try_from(metrics.len().saturating_sub(1)).unwrap_or(u32::MAX);
                    runs.push(VerticalRun {
                        first,
                        last: first.saturating_add(span),
                        metrics: RunMetrics::Explicit(metrics),
                    });
                }
                continue;
            }
            let last = items.read::<u32>(second.value())?;
            let mut value = || {
                entries
                    .next()
                    .ok_or(GlyphWidthsMapError::MissingWidthForCIDRange { c_first: first })
            };
            let (w1y, vx, vy) = (value()?, value()?, value()?);
            if last < first {
                return Err(GlyphWidthsMapError::InvalidCIDRange {
                    c_first: first,
                    c_last: last,
                }
                .into());
            }
            runs.push(VerticalRun {
                first,
                last,
                metrics: RunMetrics::Uniform(VerticalMetric {
                    w1y: items.read(w1y)?,
                    vx: items.read(vx)?,
                    vy: items.read(vy)?,
                }),
            });
        }
        Ok(Self(runs))
    }
}

/// Vertical metrics for the inclusive CID range `first..=last`.
#[derive(Debug, Clone)]
struct VerticalRun {
    first: u32,
    last: u32,
    metrics: RunMetrics,
}

/// The metrics of one `/W2` run.
#[derive(Debug, Clone)]
enum RunMetrics {
    /// `c_first c_last w1y v1x v1y`: one metric for every CID in the run.
    Uniform(VerticalMetric),
    /// `c [w1y v1x v1y ...]`: one metric per consecutive CID.
    Explicit(Box<[VerticalMetric]>),
}

impl VerticalRun {
    /// Returns the metric `offset` CIDs past the start of the run.
    fn metric(&self, offset: u32) -> Option<VerticalMetric> {
        match &self.metrics {
            RunMetrics::Uniform(metric) => Some(*metric),
            RunMetrics::Explicit(metrics) => metrics.get(usize::try_from(offset).ok()?).copied(),
        }
    }
}

impl CodeRange for VerticalRun {
    fn first(&self) -> u32 {
        self.first
    }

    fn last(&self) -> u32 {
        self.last
    }
}
