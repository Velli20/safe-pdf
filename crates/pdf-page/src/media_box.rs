use pdf_object::{
    dictionary::Dictionary, object_collection::ObjectCollection, traits::FromDictionary,
};

use crate::error::PageError;

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
        self.bottom - self.top
    }
}

impl FromDictionary for MediaBox {
    const KEY: &'static str = "MediaBox";
    type ResultType = Option<MediaBox>;
    type ErrorType = PageError;

    fn from_dictionary(
        dictionary: &Dictionary,
        _objects: &ObjectCollection,
    ) -> Result<Self::ResultType, PageError> {
        let Some(array) = dictionary.get_array(Self::KEY) else {
            return Ok(None);
        };

        match array.as_slice() {
            // Pattern match for exactly 4 elements in the slice.
            [l, t, r, b] => {
                // Safely extract and cast the values
                let left = l.as_number::<u32>()?;
                let top = t.as_number::<u32>()?;
                let right = r.as_number::<u32>()?;
                let bottom = b.as_number::<u32>()?;

                return Ok(Some(MediaBox::new(left, top, right, bottom)));
            }
            _ => {
                return Err(PageError::InvalidMediaBox(
                    "MediaBox array must contain exactly 4 numbers",
                ));
            }
        }
    }
}
