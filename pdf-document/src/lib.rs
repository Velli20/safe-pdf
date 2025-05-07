use pdf_object::{Value, object_collection::ObjectCollection, trailer::Trailer, version::Version};
use pdf_page::PdfPage;
use pdf_parser::{ParseObject, PdfParser};

/// Represents a PDF document.
pub struct PdfDocument {
    /// The version of the PDF document.
    pub version: Version,
    /// The objects in the PDF document.
    pub objects: ObjectCollection,

    pub pages: Vec<PdfPage>,

    trailer: Trailer,
}

impl PdfDocument {
    pub fn from(input: &[u8]) -> Self {
        let mut parser = PdfParser::from(input);
        let version: Version = parser.parse().unwrap();

        let mut trailer = None;
        let mut objects = ObjectCollection::default();
        loop {
            let object = parser.parse_object().unwrap();

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

        let trailer = trailer.unwrap();

        // Get the `Root` object reference.
        let root = trailer.dictionary.get_object("Root").unwrap();

        // Get the catalog.
        let catalog = objects.get(root.object_number).unwrap();
        let catalog = if let Value::Dictionary(d) = catalog {
            d.clone()
        } else {
            panic!()
        };

        let pages_num = catalog.get_object("Pages").unwrap();
        println!("pages_num {}", pages_num.object_number);

        let pages_dict = objects.get(pages_num.object_number).unwrap();
        let pages_dict = if let Value::Dictionary(d) = pages_dict {
            d.clone()
        } else {
            panic!()
        };

        println!("Got Pages");
        let kids = pages_dict.get_array("Kids").unwrap();

        let mut pages = vec![];
        for c in &kids.0 {
            if let Value::IndirectObject(p) = c.as_ref() {
                println!("Page {}", p.object_number);
                let page_obj = objects.get(p.object_number).unwrap();

                let page_obj = if let Value::Dictionary(d) = page_obj {
                    d.clone()
                } else {
                    panic!()
                };

                let page = PdfPage::from_dictionary(&page_obj, &objects);
                pages.push(page);
            } else {
                panic!("SS");
            }
        }

        PdfDocument {
            version,
            objects,
            pages,
            trailer,
        }
    }
}
