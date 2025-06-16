use crate::page::PdfPage;
use pdf_object::{
    dictionary::Dictionary, object_collection::ObjectCollection, traits::FromDictionary,
};

use thiserror::Error;

/// Errors that can occur during parsing of a PDF Pages object.
#[derive(Error, Debug)]
pub enum PdfPagesError {
    /// Missing required `/Kids` array.
    #[error("Missing required `/Kids` array in Pages object")]
    MissingKidsArray,

    /// An entry in the `/Kids` array was not an indirect object reference as expected.
    #[error(
        "Invalid entry in `/Kids` array: expected an indirect object reference, found {found_type}"
    )]
    InvalidKidEntry { found_type: &'static str },

    /// A page object referenced in `/Kids` array could not be found or is not a dictionary.
    #[error("Page object with number {obj_num} not found or is not a dictionary")]
    PageObjectNotFound { obj_num: i32 },

    /// A page or pages dictionary within the `/Kids` array is missing the required `/Type` entry.
    #[error("Missing required `/Type` entry in dictionary for object {obj_num}")]
    MissingTypeEntryInKid { obj_num: i32 },

    /// An object referenced in `/Kids` array has an unexpected `/Type`.
    #[error(
        "Unexpected object type in `/Kids` array for object {obj_num}: expected 'Page' or 'Pages', found '{found_type}'"
    )]
    UnexpectedObjectTypeInKids { obj_num: i32, found_type: String },

    /// An error occurred while processing a child `PdfPage` object.
    #[error("Error processing child Page object (obj {obj_num}):")]
    PageProcessingError {
        obj_num: i32, /* , #[source] source: PageError  */
    },
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
        // Get the `Kids` array from the `Pages` object, which contains references to the individual pages.
        let kids_array = dictionary
            .get_array("Kids")
            .ok_or(PdfPagesError::MissingKidsArray)?;

        // Iterate over the `Kids` array and extract the individual page objects.
        let mut pages = vec![];
        for kid_value in kids_array {
            let kid_ref = kid_value
                .as_object()
                .ok_or_else(|| PdfPagesError::InvalidKidEntry {
                    found_type: kid_value.name(),
                })?;

            let kid_obj_num = kid_ref.object_number();

            // Get the page object dictionary.
            let kid_dict =
                objects
                    .get_dictionary(kid_obj_num)
                    .ok_or(PdfPagesError::PageObjectNotFound {
                        obj_num: kid_obj_num,
                    })?;

            let object_type =
                kid_dict
                    .get_string("Type")
                    .ok_or(PdfPagesError::MissingTypeEntryInKid {
                        obj_num: kid_obj_num,
                    })?;

            if object_type == PdfPage::KEY {
                let page = PdfPage::from_dictionary(kid_dict, objects).map_err(|source| {
                    PdfPagesError::PageProcessingError {
                        obj_num: kid_obj_num, /* , source */
                    }
                })?;
                pages.push(page);
            } else if object_type == Self::KEY {
                let pages_obj = PdfPages::from_dictionary(kid_dict, objects)?;
                pages.extend(pages_obj.pages);
            } else {
                return Err(PdfPagesError::UnexpectedObjectTypeInKids {
                    obj_num: kid_obj_num,
                    found_type: object_type.to_string(),
                });
            }
        }

        Ok(Self { pages })
    }
}
