use std::fmt::write;

#[derive(Debug)]
pub enum PdfCanvasError {
    NoActivePath,
    NoCurrentPoint,
    NoCurrentFont,
}

impl std::fmt::Display for PdfCanvasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PdfCanvasError::NoActivePath => {
                write!(f, "No active path to perform the painting operation.")
            }
            PdfCanvasError::NoCurrentPoint => {
                write!(f, "Operation requires a current point, but none is set.")
            }
            PdfCanvasError::NoCurrentFont => {
                write!(f, "Operation requires a current font, but none is set.")
            }
        }
    }
}
