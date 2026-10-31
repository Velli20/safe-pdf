use crate::segment::SegmentNumber;

/// Result returned by fallible JBIG2 operations; errors own their diagnostics.
pub type Result<T> = std::result::Result<T, Jbig2Error>;

/// Identifies the input buffer to which a diagnostic offset applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputOrigin {
    /// A standalone JBIG2 file including its file header.
    Standalone,
    /// An embedding application's page segment buffer.
    Embedded,
    /// The separate PDF JBIG2Globals segment buffer.
    Globals,
}

/// A position within a caller-owned input, without borrowing that input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    /// Buffer whose beginning defines offset zero.
    pub origin: InputOrigin,
    /// Byte offset from the beginning of that buffer.
    pub byte_offset: usize,
    /// Optional bit position within the byte, in the range zero through seven.
    pub bit_offset: Option<u8>,
}

/// Resource category charged against a document's shared decoding budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitKind {
    /// Total encoded input bytes.
    InputBytes,
    /// Parsed segment count.
    Segments,
    /// Total referred-to segment entries.
    References,
    /// Total decoded bitmap pixels.
    Pixels,
    /// Cumulative requested allocation bytes.
    AllocationBytes,
    /// Total decoded symbols.
    Symbols,
    /// Abstract entropy/placement work units.
    Work,
}

/// Owned, extensible failure diagnostics suitable for translation by adapters.
///
/// No variant borrows input or exposes pointers. Foreign adapters must map this
/// Rust enum into their own ABI rather than passing its memory representation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Jbig2Error {
    /// More bytes were required to complete a bounded parsing operation.
    #[error("truncated input at {location:?}: need {needed} bytes")]
    Truncated {
        /// Position of the incomplete field.
        location: SourceLocation,
        /// Additional bytes required.
        needed: usize,
    },
    /// A segment or file header violates its syntax.
    #[error("malformed header at {location:?}: {field}")]
    MalformedHeader {
        /// Position of the offending field.
        location: SourceLocation,
        /// Static specification field name.
        field: &'static str,
    },
    /// A reserved bit or value was encountered.
    #[error("reserved field at {location:?}: {field}")]
    ReservedField {
        /// Position of the reserved field.
        location: SourceLocation,
        /// Static specification field name.
        field: &'static str,
    },
    /// A declared or inferred segment extent is invalid.
    #[error("invalid segment length at {0:?}")]
    InvalidLength(SourceLocation),
    /// A referred-to resource is unavailable.
    #[error("missing segment {0:?}")]
    MissingReference(SegmentNumber),
    /// A segment number is defined more than once in the decoding namespace.
    #[error("duplicate segment {0:?}")]
    DuplicateSegment(SegmentNumber),
    /// Dependency traversal revisited an active segment.
    #[error("dependency cycle at {0:?}")]
    DependencyCycle(SegmentNumber),
    /// A referred-to segment has an incompatible decoded resource kind.
    #[error("wrong resource type for {0:?}")]
    WrongResourceType(SegmentNumber),
    /// Entropy data, templates, tables, or symbol indices are invalid.
    #[error("invalid coding data at {location:?}: {reason}")]
    InvalidCodingData {
        /// Position of the invalid coded data.
        location: SourceLocation,
        /// Static description of the violated invariant.
        reason: &'static str,
    },
    /// A symbol refers to another that its dictionary has not decoded yet.
    #[error("symbol {0} is not decoded yet")]
    UnresolvedSymbol(u32),
    /// A decoded table, template, or index is invalid; no input position applies.
    #[error("invalid decoded resource: {0}")]
    InvalidResource(&'static str),
    /// Page or stripe ordering violates the document lifecycle.
    #[error("invalid page sequence: {0}")]
    InvalidPageSequence(&'static str),
    /// The bitmap geometry, stride, or storage length is inconsistent.
    #[error("invalid bitmap: {0}")]
    InvalidBitmap(&'static str),
    /// A recognized feature has no supported decoding path.
    #[error("unsupported JBIG2 feature: {0}")]
    UnsupportedFeature(&'static str),
    /// Checked arithmetic or conversion cannot represent a required value.
    #[error("integer overflow: {0}")]
    Overflow(&'static str),
    /// An explicitly supplied decoding budget was exceeded.
    #[error("resource limit exceeded: {0:?}")]
    ResourceLimit(LimitKind),
    /// Fallible storage reservation failed.
    #[error("allocation failed: {0}")]
    Allocation(#[from] std::collections::TryReserveError),
    /// An embedding application's I/O adapter failed.
    #[error("input/output failure: {0}")]
    Io(#[from] std::io::Error),
}
