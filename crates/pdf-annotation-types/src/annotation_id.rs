/// A stable, page-scoped annotation identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AnnotationId(usize);

impl AnnotationId {
    /// Returns the numeric value of this runtime-only identifier.
    pub const fn get(self) -> usize {
        self.0
    }

    /// Creates an identifier from a page-local numeric value.
    #[doc(hidden)]
    pub const fn from_page_value(value: usize) -> Self {
        Self(value)
    }
}
