//! Inclusive code ranges and lookup over sorted, disjoint range lists.

/// An inclusive range of packed source codes or CIDs.
pub trait CodeRange {
    /// First code covered by the range.
    fn first(&self) -> u32;
    /// Last code covered by the range, inclusive.
    fn last(&self) -> u32;
}

/// Lookup over ranges sorted by first code without overlaps.
pub trait CodeRanges<R> {
    /// Returns the range containing `code` and the offset of `code` within it.
    fn covering(&self, code: u32) -> Option<(&R, u32)>;
}

impl<R: CodeRange> CodeRanges<R> for [R] {
    fn covering(&self, code: u32) -> Option<(&R, u32)> {
        let range = self.get(self.partition_point(|range| range.last() < code))?;
        Some((range, code.checked_sub(range.first())?))
    }
}

/// Sorts ranges by first code and drops each range that overlaps an earlier one, so the result
/// satisfies [`CodeRanges`]. On overlap, the lower-starting (then earlier-listed) range wins.
pub fn disjoint<R: CodeRange>(mut ranges: Vec<R>) -> Vec<R> {
    ranges.sort_by_key(CodeRange::first);
    ranges.dedup_by(|next, kept| next.first() <= kept.last());
    ranges
}
