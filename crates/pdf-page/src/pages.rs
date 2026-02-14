use crate::{page::PdfPage, resource_cache::ResourceCache, resources::ResourcesError};
use pdf_content_stream::error::PdfOperatorError;
use pdf_object::{dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver};

use thiserror::Error;

/// Errors that can occur during parsing of a PDF Pages object.
#[derive(Error, Debug)]
pub enum PdfPagesError {
    #[error(
        "Unexpected object type in `/Kids` array for an object: expected 'Page' or 'Pages', found '{found_type}'"
    )]
    UnexpectedObjectTypeInKids { found_type: String },
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("Failed to parse content stream for page: {0}")]
    ContentStreamParse(#[from] PdfOperatorError),
    #[error("Failed to parse resources for page: {0}")]
    ResourcesParse(#[from] ResourcesError),
}

pub struct PdfPages;

impl PdfPages {
    pub const KEY: &'static str = "Pages";

    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
    ) -> Result<Vec<PdfPage>, PdfPagesError> {
        // The `/Kids` array is a required entry in a Pages dictionary. It contains
        // indirect references to child objects, which can be either other Pages nodes
        // or leaf Page nodes.
        let kids_array = dictionary.get_or_err("Kids")?.try_array(objects)?;

        // This vector will store the flattened list of all leaf `PdfPage` objects
        // found by traversing the page tree.
        let mut pages = vec![];

        // Iterate over each entry in the `/Kids` array.
        for value in kids_array {
            // Resolve the indirect reference to get the child's dictionary.
            let dictionary = value.try_dictionary(objects)?;

            // Determine the type of the child object by reading its `/Type` entry.
            match dictionary.get_or_err("Type")?.try_str(objects)?.as_ref() {
                PdfPage::KEY => {
                    // If the child is a leaf node (`/Type /Page`), parse it as a `PdfPage`.
                    let page = PdfPage::from_dictionary(dictionary, objects, cache)?;
                    pages.push(page);
                }
                PdfPages::KEY => {
                    // If the child is another branch node (`/Type /Pages`), recursively call this
                    // function to process its children and extend our list of pages.
                    pages.extend(PdfPages::from_dictionary(dictionary, objects, cache)?);
                }
                obj_type => {
                    // If the child has an unexpected type, return an error.
                    return Err(PdfPagesError::UnexpectedObjectTypeInKids {
                        found_type: obj_type.to_string(),
                    });
                }
            }
        }

        Ok(pages)
    }
}
