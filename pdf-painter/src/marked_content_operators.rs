use crate::PdfOperator;

/// Begins a marked-content sequence. (PDF operator `BMC`)
/// Marked-content sequences associate a tag with a sequence of content stream operations.
#[derive(Debug, Clone, PartialEq)]
pub struct BeginMarkedContent {
    /// The tag indicating the role or nature of the marked-content sequence.
    tag: String,
}

impl PdfOperator for BeginMarkedContent {
    fn operator() -> &'static str {
        "BMC"
    }
}

impl BeginMarkedContent {
    pub fn new(tag: String) -> Self {
        Self { tag }
    }
}

/// Begins a marked-content sequence with an associated property list. (PDF operator `BDC`)
/// The property list can be either a name (referring to an entry in the Properties subdictionary
/// of the current resource dictionary) or an inline dictionary.
#[derive(Debug, Clone, PartialEq)]
pub struct BeginMarkedContentWithProps {
    /// The tag indicating the role or nature of the marked-content sequence.
    tag: String,
    /// The property list, which can be a name (of an entry in the resource dictionary's Properties subdictionary) or an inline dictionary.
    properties: String,
}

impl PdfOperator for BeginMarkedContentWithProps {
    fn operator() -> &'static str {
        "BDC"
    }
}

impl BeginMarkedContentWithProps {
    pub fn new(tag: String, properties: String) -> Self {
        Self { tag, properties }
    }
}

/// Ends a marked-content sequence begun by a `BMC` or `BDC` operator. (PDF operator `EMC`)
#[derive(Debug, Clone, PartialEq)]
pub struct EndMarkedContent;

impl PdfOperator for EndMarkedContent {
    fn operator() -> &'static str {
        "EMC"
    }
}

impl EndMarkedContent {
    pub fn new() -> Self {
        Self
    }
}
