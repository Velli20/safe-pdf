use crate::{
    content_stream::ContentStreamReadError, media_box::MediaBoxError, page::PdfPage,
    resources::ResourcesError,
};
use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};

use thiserror::Error;

/// Errors that can occur during parsing of a PDF Pages object.
#[derive(Error, Debug)]
pub enum PdfPagesError {
    #[error(
        "Unexpected object type in `/Kids` array for object {obj_num}: expected 'Page' or 'Pages', found '{found_type}'"
    )]
    UnexpectedObjectTypeInKids { obj_num: i32, found_type: String },
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("Failed to parse content stream for page: {0}")]
    ContentStreamParse(#[from] ContentStreamReadError),
    #[error("Failed to parse media box for page: {0}")]
    MediaBoxParse(#[from] MediaBoxError),
    #[error("Failed to parse resources for page: {0}")]
    ResourcesParse(#[from] ResourcesError),
}

pub struct PdfPages {
    pub pages: Vec<PdfPage>,
}

impl FromDictionary for PdfPages {
    const KEY: &'static str = "Pages";

    type ResultType = Self;
    type ErrorType = PdfPagesError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        // The `/Kids` array is a required entry in a Pages dictionary. It contains
        // indirect references to child objects, which can be either other Pages nodes
        // or leaf Page nodes.
        let kids_array = dictionary.get_or_err("Kids")?.try_array()?;

        // This vector will store the flattened list of all leaf `PdfPage` objects
        // found by traversing the page tree.
        let mut pages = vec![];

        // Iterate over each entry in the `/Kids` array.
        for value in kids_array {
            // Each entry must be an indirect reference. We extract its object number
            // for use in error messages.
            let obj_num = value.try_object_number()?;

            // Resolve the indirect reference to get the child's dictionary.
            let dictionary = objects.resolve_dictionary(value)?;

            // Determine the type of the child object by reading its `/Type` entry.
            match dictionary.get_or_err("Type")?.try_str()?.as_ref() {
                PdfPage::KEY => {
                    // If the child is a leaf node (`/Type /Page`), parse it as a `PdfPage`.
                    let page = PdfPage::from_dictionary(dictionary, objects)?;
                    pages.push(page);
                }
                PdfPages::KEY => {
                    // If the child is another branch node (`/Type /Pages`), recursively call this
                    // function to process its children and extend our list of pages.
                    let pages_obj = PdfPages::from_dictionary(dictionary, objects)?;
                    pages.extend(pages_obj.pages);
                }
                obj_type => {
                    // If the child has an unexpected type, return an error.
                    return Err(PdfPagesError::UnexpectedObjectTypeInKids {
                        obj_num,
                        found_type: obj_type.to_string(),
                    });
                }
            }
        }

        Ok(Self { pages })
    }
}
