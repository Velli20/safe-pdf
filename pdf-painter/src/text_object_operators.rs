use crate::PdfOperator;

/// Begins a text object, initializing the text matrix and text line matrix to the identity matrix. (PDF operator `BT`)
#[derive(Debug, Clone, PartialEq)]
pub struct BeginText;

impl PdfOperator for BeginText {
    fn operator() -> &'static str {
        "BT"
    }
}

impl BeginText {
    pub fn new() -> Self {
        Self
    }
}

/// Ends a text object, discarding the text matrix and text line matrix. (PDF operator `ET`)
#[derive(Debug, Clone, PartialEq)]
pub struct EndText;

impl PdfOperator for EndText {
    fn operator() -> &'static str {
        "ET"
    }
}

impl EndText {
    pub fn new() -> Self {
        Self
    }
}
