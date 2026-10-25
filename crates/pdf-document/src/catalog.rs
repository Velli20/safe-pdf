//! Decodes the document catalog referenced by the trailer's `/Root` entry.

use pdf_object_collection::object_collection::ObjectCollection;
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ObjectReader, ReadResult, object_id::ObjectId,
};
use pdf_optional_content::OptionalContentProperties;

use crate::diagnostic::{PdfReadDiagnostic, PdfReadDiagnosticKind};
use crate::document::PdfDocument;
use crate::page_tree::PageTree;

/// The catalog entries a document is assembled from.
pub(crate) struct Catalog {
    /// The page tree rooted at `/Pages`.
    pages: PageTree,
    /// The `/OCProperties` entry, if present.
    optional_content: Option<OptionalContentEntry>,
}

/// An `/OCProperties` declaration, kept with its read failure so it can be reported.
struct OptionalContentEntry {
    /// The indirect object holding the declaration, if it is not direct.
    object: Option<ObjectId>,
    /// The decoded declaration or the error that prevented decoding it.
    properties: ReadResult<OptionalContentProperties>,
}

impl FromPdfObject for Catalog {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut catalog = context.dictionary()?;
        let pages = catalog.required::<PageTree>(b"Pages")?;
        let optional_content = catalog
            .contains(b"OCProperties")
            .then(|| OptionalContentEntry {
                object: catalog.reference(b"OCProperties"),
                properties: catalog.required::<OptionalContentProperties>(b"OCProperties"),
            });
        Ok(Self {
            pages,
            optional_content,
        })
    }
}

impl Catalog {
    /// Builds the document, handing it the reader its pages' resources were decoded through.
    ///
    /// Optional content only governs presentation, so an unreadable declaration is
    /// reported and dropped rather than failing an otherwise usable document.
    pub(crate) fn into_document(
        self,
        objects: ObjectReader<ObjectCollection>,
        diagnostics: &mut Vec<PdfReadDiagnostic>,
    ) -> PdfDocument {
        let optional_content = self
            .optional_content
            .and_then(|entry| match entry.properties {
                Ok(properties) => Some(properties),
                Err(error) => {
                    diagnostics.push(PdfReadDiagnostic::new(
                        PdfReadDiagnosticKind::ObjectParse,
                        None,
                        entry.object,
                        error,
                    ));
                    None
                }
            });
        PdfDocument::new(self.pages.pages, optional_content, objects)
    }
}
