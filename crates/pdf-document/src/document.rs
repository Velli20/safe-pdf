use pdf_object_collection::object_collection::ObjectCollection;
use pdf_object_reader::ObjectReader;
use pdf_optional_content::OptionalContentProperties;

use crate::page::PdfPage;

/// A decoded PDF document with page attributes and resources resolved.
pub struct PdfDocument {
    /// The document's pages, in source order.
    pub pages: Vec<PdfPage>,
    /// The catalog's `/OCProperties`, absent when the document declares no optional
    /// content or its declaration could not be read.
    pub optional_content: Option<OptionalContentProperties>,
    /// Owns the typed object cache that the pages' indirect resource handles point into;
    /// dropping it would expire those handles.
    objects: ObjectReader<ObjectCollection>,
}

impl PdfDocument {
    /// Creates a document whose pages' resources were decoded through `objects`.
    pub(crate) fn new(
        pages: Vec<PdfPage>,
        optional_content: Option<OptionalContentProperties>,
        objects: ObjectReader<ObjectCollection>,
    ) -> Self {
        Self {
            pages,
            optional_content,
            objects,
        }
    }

    /// Returns the number of pages in the document.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Returns the page at the given zero-based position, if present.
    pub fn get_page(&self, index: usize) -> Option<&PdfPage> {
        self.pages.get(index)
    }

    /// Returns the parsed indirect objects the document was read from.
    pub fn objects(&self) -> &ObjectCollection {
        self.objects.source()
    }
}
