use error::PdfError;
use pdf_object::{
    ObjectVariant, object_collection::ObjectCollection, trailer::Trailer, traits::FromDictionary,
    version::Version,
};
use pdf_page::{page::PdfPage, pages::PdfPages};
use pdf_parser::{PdfParser, traits::HeaderParser};

pub mod error;

/// Represents a PDF document.
pub struct PdfDocument {
    /// The version of the PDF document.
    pub version: Version,
    /// The objects in the PDF document.
    pub objects: ObjectCollection,

    pub pages: Vec<PdfPage>,
    /// The trailer of the PDF document.
    trailer: Trailer,
}

impl PdfDocument {
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn get_page(&self, index: usize) -> Option<&PdfPage> {
        self.pages.get(index)
    }

    pub fn from(input: &[u8]) -> Result<Self, PdfError> {
        let mut parser = PdfParser::from(input);
        let version = parser.parse_header()?;

        let mut trailer = None;
        let mut objects = ObjectCollection::default();
        loop {
            let object = parser.parse_object()?;

            match object {
                ObjectVariant::EndOfFile => break,
                ObjectVariant::IndirectObject(_)
                | ObjectVariant::Reference(_)
                | ObjectVariant::Stream(_) => objects.insert(object)?,

                ObjectVariant::Trailer(t) => {
                    trailer = Some(t);
                }
                ObjectVariant::CrossReferenceTable(_) => {}
                _ => {}
            }
        }

        let trailer = trailer.ok_or(PdfError::MissingTrailer)?;

        // Get the `Root` object reference.
        let root = trailer.dictionary.get_or_err("Root")?;
        // Get the catalog.
        let catalog = objects.resolve_dictionary(root)?;

        // Get the `Pages` object reference from the catalog, which defines the order of the pages in the document.
        let pages_num = catalog.get_or_err("Pages")?;
        let pages_dict = objects.resolve_dictionary(pages_num)?;

        let pages = PdfPages::from_dictionary(pages_dict, &objects)?;

        Ok(PdfDocument {
            version,
            objects,
            pages: pages.pages,
            trailer,
        })
    }
}
