//! Decoder for the `c [v ...]` / `c_first c_last v` grammar of a CID font's `/W` and `/W2`.

use crate::{glyph_widths_map::GlyphWidthsMapError, metric_run::MetricRun};
use pdf_object_reader::{
    ArrayContext, FromPdfObject, ObjectAccess, ObjectContext, ReadResult,
    object_variant::ObjectVariant,
};

/// The runs of a CID font's `/W` (`N = 1`) or `/W2` (`N = 3`) array, omitting empty ones.
pub(crate) struct CidMetricRuns<const N: usize>(pub(crate) Vec<MetricRun<N>>);

impl<const N: usize> FromPdfObject for CidMetricRuns<N> {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        RunEntries {
            items: context.array()?,
            next: 0,
        }
        .filter_map(Result::transpose)
        .collect::<ReadResult<_>>()
        .map(Self)
    }
}

/// What follows `c_first` in an entry: its value array, or `c_last` with the shared value
/// after it.
enum RunTail<const N: usize> {
    /// `c_first [v1 ... vn]`: one value per consecutive code; a trailing partial value is
    /// ignored.
    Explicit(Box<[[f32; N]]>),
    /// `c_first c_last v`: the last code of a range sharing one value.
    Last(u32),
}

impl<const N: usize> FromPdfObject for RunTail<N> {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let ObjectVariant::Array(_) = context.object() else {
            return context.decode().map(Self::Last);
        };
        let mut numbers = context.array()?;
        let count = numbers.array().len().checked_div(N).unwrap_or_default();
        (0..count)
            .map(|value| numbers.read_array_from(value.saturating_mul(N)))
            .collect::<ReadResult<_>>()
            .map(Self::Explicit)
    }
}

/// Reads the entries of a `/W` or `/W2` array in order, one run per entry.
///
/// An empty `c_first [ ]` entry yields `None`.
struct RunEntries<'read, A: ObjectAccess + ?Sized, const N: usize> {
    items: ArrayContext<'read, A>,
    /// Index of the next entry's `c_first`.
    next: usize,
}

impl<A: ObjectAccess + ?Sized, const N: usize> Iterator for RunEntries<'_, A, N> {
    type Item = ReadResult<Option<MetricRun<N>>>;

    fn next(&mut self) -> Option<Self::Item> {
        (self.next < self.items.array().len()).then(|| self.read_entry())
    }
}

impl<A: ObjectAccess + ?Sized, const N: usize> RunEntries<'_, A, N> {
    /// Reads the entry at `next`: `c_first`, then the tail that selects its form.
    fn read_entry(&mut self) -> ReadResult<Option<MetricRun<N>>> {
        let first = self.items.at::<u32>(self.next)?;
        let tail = self
            .items
            .optional_at::<RunTail<N>>(self.next.saturating_add(1))?
            .ok_or(GlyphWidthsMapError::IncompleteCIDEntry { cid: first })?;
        match tail {
            RunTail::Explicit(values) => {
                self.next = self.next.saturating_add(2);
                Ok((!values.is_empty()).then(|| MetricRun::explicit(first, values)))
            }
            RunTail::Last(last) => self.read_uniform(first, last).map(Some),
        }
    }

    /// Reads the value after `c_first c_last`, shared by every code in the range.
    fn read_uniform(&mut self, first: u32, last: u32) -> ReadResult<MetricRun<N>> {
        let start = self.next.saturating_add(2);
        let end = start.saturating_add(N);
        if self.items.array().len() < end {
            return Err(GlyphWidthsMapError::MissingWidthForCIDRange { c_first: first }.into());
        }
        let value = self.items.read_array_from(start)?;
        if last < first {
            return Err(GlyphWidthsMapError::InvalidCIDRange {
                c_first: first,
                c_last: last,
            }
            .into());
        }
        self.next = end;
        Ok(MetricRun::uniform(first, last, value))
    }
}
