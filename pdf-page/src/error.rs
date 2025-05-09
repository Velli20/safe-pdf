/// Defines errors that can occur when interpreting a PDF page object.
#[derive(Debug, Clone, PartialEq)]
pub enum PageError {
    /// The page dictionary is missing `/Contents` entry.
    MissingContent,
    /// The page is missing required `/MediaBox` entry.
    MissingMediaBox,
    /// The `/MediaBox` entry in the page dictionary is invalid.
    InvalidMediaBox(&'static str),
}

impl std::fmt::Display for PageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PageError::MissingContent => {
                write!(f, "Missing `/Contents` entry")
            }
            PageError::MissingMediaBox => {
                write!(f, "Missing `/Contents` entry")
            }
            PageError::InvalidMediaBox(err) => {
                write!(f, "Invalid `/MediaBox` entry: {}", err)
            }
        }
    }
}
