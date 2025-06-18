use pdf_object::error::ObjectError;
use pdf_object::{
    ObjectVariant, dictionary::Dictionary, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

/// Defines the page boundaries within a PDF document.
///
/// The `MediaBox` is a rectangle, expressed in default user space units,
/// that defines the boundaries of the physical medium on which the page
/// is intended to be displayed or printed.
#[derive(Default, Debug, Clone)]
pub struct MediaBox {
    /// The x-coordinate of the lower-left corner of the rectangle.
    pub left: u32,
    /// The y-coordinate of the upper-right corner of the rectangle.
    pub top: u32,
    /// The x-coordinate of the upper-right corner of the rectangle.
    pub right: u32,
    /// The y-coordinate of the lower-left corner of the rectangle.
    pub bottom: u32,
}

impl MediaBox {
    pub fn new(left: u32, top: u32, right: u32, bottom: u32) -> Self {
        MediaBox {
            left,
            top,
            right,
            bottom,
        }
    }

    pub fn width(&self) -> u32 {
        self.right - self.left
    }

    pub fn height(&self) -> u32 {
        self.top - self.bottom
    }
}

/// Defines errors that can occur while parsing a MediaBox.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum MediaBoxError {
    #[error("MediaBox entry is not an array, found type {found_type}")]
    NotAnArray { found_type: &'static str },
    #[error("MediaBox array must contain exactly 4 elements, found {found_length}")]
    InvalidArrayLength { found_length: usize },
    #[error("MediaBox array element at index {index} is not a number: found type {found_type}")]
    ElementTypeNotNumber {
        index: usize,
        found_type: &'static str,
    },
    #[error("MediaBox array element at index {index} could not be converted to u32")]
    NumberConversionFailed {
        index: usize,
        #[source]
        source: ObjectError,
    },
}

impl FromDictionary for MediaBox {
    const KEY: &'static str = "MediaBox";
    type ResultType = Option<MediaBox>;
    type ErrorType = MediaBoxError;

    fn from_dictionary(
        dictionary: &Dictionary,
        _objects: &ObjectCollection,
    ) -> Result<Self::ResultType, MediaBoxError> {
        let Some(media_box_obj) = dictionary.get(Self::KEY) else {
            // MediaBox can be inherited; if not present directly, it's not an error here.
            return Ok(None);
        };

        // Helper closure to parse each coordinate from an ObjectVariant
        let parse_coord = |obj: &ObjectVariant, idx: usize| -> Result<u32, MediaBoxError> {
            obj.as_number::<u32>()
                .map_err(|err| MediaBoxError::NumberConversionFailed {
                    index: idx,
                    source: err,
                })
        };

        match media_box_obj.as_array() {
            Some(array_elements) => {
                // PDF MediaBox is an array of four numbers: [LLx, LLy, URx, URy]
                match array_elements {
                    [llx_obj, lly_obj, urx_obj, ury_obj] => {
                        let left = parse_coord(llx_obj, 0)?;
                        let bottom = parse_coord(lly_obj, 1)?;
                        let right = parse_coord(urx_obj, 2)?;
                        let top = parse_coord(ury_obj, 3)?;

                        Ok(Some(MediaBox::new(left, top, right, bottom)))
                    }
                    _ => Err(MediaBoxError::InvalidArrayLength {
                        found_length: array_elements.len(),
                    }),
                }
            }
            None => Err(MediaBoxError::NotAnArray {
                found_type: media_box_obj.name(),
            }),
        }
    }
}
