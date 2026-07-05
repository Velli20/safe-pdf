use crate::page::PdfPage;

/// Represents a PDF document.
pub struct PdfDocument {
    /// The pages in the PDF document.
    pub pages: Vec<PdfPage>,
}

impl PdfDocument {
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn get_page(&self, index: usize) -> Option<&PdfPage> {
        self.pages.get(index)
    }
}
