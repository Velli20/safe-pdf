use crate::page::{PdfPage, PdfPageError};
use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};

use thiserror::Error;

/// Errors that can occur during parsing of a PDF Pages object.
#[derive(Error, Debug)]
pub enum PdfPagesError {
    #[error("Missing required `/Kids` array in Pages object")]
    MissingKidsArray,
    #[error(
        "Invalid entry in `/Kids` array: expected an indirect object reference, found {found_type}"
    )]
    InvalidKidEntry { found_type: &'static str },
    #[error("Missing required `/Type` entry in dictionary for object {obj_num}")]
    MissingTypeEntryInKid { obj_num: i32 },
    #[error(
        "Unexpected object type in `/Kids` array for object {obj_num}: expected 'Page' or 'Pages', found '{found_type}'"
    )]
    UnexpectedObjectTypeInKids { obj_num: i32, found_type: String },
    #[error("Error processing child Page object (obj {obj_num}): {source}")]
    PageProcessingError {
        obj_num: i32,
        #[source]
        source: PdfPageError,
    },
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
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
        let kids_array = dictionary
            .get_array("Kids")
            .ok_or(PdfPagesError::MissingKidsArray)?;

        // This vector will store the flattened list of all leaf `PdfPage` objects
        // found by traversing the page tree.
        let mut pages = vec![];

        // Iterate over each entry in the `/Kids` array.
        for value in kids_array {
            // Each entry must be an indirect reference. We extract its object number
            // for use in error messages.
            let obj_num =
                value
                    .as_object_number()
                    .ok_or_else(|| PdfPagesError::InvalidKidEntry {
                        found_type: value.name(),
                    })?;

            // Resolve the indirect reference to get the child's dictionary.
            let kid_dict = objects.resolve_dictionary(value)?;

            // Determine the type of the child object by reading its `/Type` entry.
            let object_type = kid_dict
                .get_string("Type")
                .ok_or(PdfPagesError::MissingTypeEntryInKid { obj_num })?;

            // If the child is a leaf node (`/Type /Page`), parse it as a `PdfPage`.
            if object_type == PdfPage::KEY {
                let page = PdfPage::from_dictionary(kid_dict, objects).map_err(|err| {
                    PdfPagesError::PageProcessingError {
                        obj_num,
                        source: err,
                    }
                })?;
                pages.push(page);
            } else if object_type == Self::KEY {
                // If the child is another branch node (`/Type /Pages`), recursively call this
                // function to process its children and extend our list of pages.
                let pages_obj = PdfPages::from_dictionary(kid_dict, objects)?;
                pages.extend(pages_obj.pages);
            } else {
                // If the child has an unexpected type, return an error.
                return Err(PdfPagesError::UnexpectedObjectTypeInKids {
                    obj_num,
                    found_type: object_type.to_string(),
                });
            }
        }

        Ok(Self { pages })
    }
}
