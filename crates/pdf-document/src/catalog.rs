//! Decodes the document catalog referenced by the trailer's `/Root` entry.

use pdf_object_collection::object_collection::ObjectCollection;
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ObjectReader, ReadResult, object_id::ObjectId,
};
use pdf_optional_content::OptionalContentProperties;

use crate::diagnostic::{PdfReadDiagnostic, PdfReadDiagnosticKind};
use crate::document::PdfDocument;
use crate::interactive_form::InteractiveForm;
use crate::page_tree::PageTree;

/// The catalog entries a document is assembled from.
pub(crate) struct Catalog {
    /// The page tree rooted at `/Pages`.
    pages: PageTree,
    /// The `/OCProperties` entry, if present.
    optional_content: Option<CatalogEntry<OptionalContentProperties>>,
    /// The `/AcroForm` entry, if present.
    interactive_form: Option<CatalogEntry<InteractiveForm>>,
}

/// An optional catalog entry, kept with its read failure so it can be reported.
struct CatalogEntry<T> {
    /// The indirect object holding the entry, if it is not direct.
    object: Option<ObjectId>,
    /// The decoded entry or the error that prevented decoding it.
    value: ReadResult<T>,
}

impl FromPdfObject for Catalog {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut catalog = context.dictionary()?;
        let pages = catalog.required::<PageTree>(b"Pages")?;
        let optional_content = catalog.contains(b"OCProperties").then(|| CatalogEntry {
            object: catalog.reference(b"OCProperties"),
            value: catalog.required(b"OCProperties"),
        });
        let interactive_form = catalog.contains(b"AcroForm").then(|| CatalogEntry {
            object: catalog.reference(b"AcroForm"),
            value: catalog.required(b"AcroForm"),
        });
        Ok(Self {
            pages,
            optional_content,
            interactive_form,
        })
    }
}

impl Catalog {
    /// Builds the document, handing it the reader its pages' resources were decoded through.
    ///
    /// Optional content and the interactive form only govern presentation, so an
    /// unreadable declaration is reported and dropped rather than failing an otherwise
    /// usable document.
    pub(crate) fn into_document(
        self,
        objects: ObjectReader<ObjectCollection>,
        diagnostics: &mut Vec<PdfReadDiagnostic>,
    ) -> PdfDocument {
        let optional_content = self
            .optional_content
            .and_then(|entry| entry.into_value(diagnostics));
        let interactive_form = self
            .interactive_form
            .and_then(|entry| entry.into_value(diagnostics));
        PdfDocument::new(
            self.pages.pages,
            optional_content,
            interactive_form,
            objects,
        )
    }
}

impl<T> CatalogEntry<T> {
    /// The decoded entry, or `None` after reporting the error that prevented decoding it.
    fn into_value(self, diagnostics: &mut Vec<PdfReadDiagnostic>) -> Option<T> {
        match self.value {
            Ok(value) => Some(value),
            Err(error) => {
                diagnostics.push(PdfReadDiagnostic::new(
                    PdfReadDiagnosticKind::ObjectParse,
                    None,
                    self.object,
                    error,
                ));
                None
            }
        }
    }
}
