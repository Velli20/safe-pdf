/// Defines errors that can occur in pdf-painter crate.
#[derive(Debug, Clone, PartialEq)]
pub enum PdfPainterError {
    UnimplementedOperation(&'static str),
}

impl std::fmt::Display for PdfPainterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PdfPainterError::UnimplementedOperation(name) => {
                write!(f, "Unimplemented operation: {}", name)
            }
        }
    }
}
