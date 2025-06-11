#[derive(Debug)]
pub enum PdfCanvasError {
    NoActivePath,
    NoCurrentPoint,
    NoCurrentFont,
    MissingPageResources,
    InvalidFont(&'static str),
    FontNotFound(String),
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
            PdfCanvasError::MissingPageResources => {
                write!(f, "Missing page resources")
            }
            PdfCanvasError::FontNotFound(name) => {
                write!(f, "Font '{}' not found", name)
            }
            PdfCanvasError::InvalidFont(err) => {
                write!(f, "Invalid font: {}", err)
            }
        }
    }
}
