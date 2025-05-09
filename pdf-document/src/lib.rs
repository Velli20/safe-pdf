use error::PdfError;
use pdf_object::{Value, object_collection::ObjectCollection, trailer::Trailer, version::Version};
use pdf_page::PdfPage;
use pdf_parser::{ParseObject, PdfParser};

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
    pub fn from(input: &[u8]) -> Result<Self, PdfError> {
        let mut parser = PdfParser::from(input);
        let version: Version = parser.parse().unwrap();

        let mut trailer = None;
        let mut objects = ObjectCollection::default();
        loop {
            let object = parser.parse_object()?;

            match object {
                Value::EndOfFile => break,
                Value::IndirectObject(v) => {
                    objects.insert(v).unwrap();
                }
                Value::Trailer(t) => {
                    trailer = Some(t);
                }
                Value::CrossReferenceTable(_) => {}
                _ => {}
            }
        }

        let trailer = trailer.ok_or(PdfError::MissingTrailer)?;

        // Get the `Root` object reference.
        let root = trailer
            .dictionary
            .get_object("Root")
            .ok_or(PdfError::MissingRoot)?;

        // Get the catalog.
        let catalog = objects
            .get_dictionary(root.object_number)
            .ok_or(PdfError::MissingCatalog)?
            .clone();

        // Get the `Pages` object reference from the catalog, which defines the order of the pages in the document.
        let pages_num = catalog.get_object("Pages").unwrap();

        let pages_dict = objects
            .get_dictionary(pages_num.object_number)
            .ok_or(PdfError::MissingPages)?
            .clone();

        // Get the `Kids` array from the `Pages` object, which contains references to the individual pages.
        let kids = pages_dict.get_array("Kids").unwrap();

        // Iterate over the `Kids` array and extract the individual page objects.
        let mut pages = vec![];
        for c in &kids.0 {
            let p = c.as_object().ok_or(PdfError::MissingPages)?;

            // Get the page object dictionary.
            let page_obj = objects
                .get_dictionary(p.object_number)
                .ok_or(PdfError::PageNotFound(p.object_number))?
                .clone();

            let page = PdfPage::from_dictionary(&page_obj, &objects);
            pages.push(page);
        }

        Ok(PdfDocument {
            version,
            objects,
            pages,
            trailer,
        })
    }
}
