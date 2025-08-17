use pdf_object::error::ObjectError;
use pdf_object::{
    dictionary::Dictionary, object_collection::ObjectCollection, traits::FromDictionary,
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
        debug_assert!(
            self.right >= self.left,
            "Right must be greater than or equal to left"
        );
        self.right - self.left
    }

    pub fn height(&self) -> u32 {
        debug_assert!(
            self.top >= self.bottom,
            "Top must be greater than or equal to bottom"
        );
        self.top - self.bottom
    }
}

/// Defines errors that can occur while parsing a MediaBox.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum MediaBoxError {
    #[error("Error parsing MediaBox: {0}")]
    ObjectError(#[from] ObjectError),
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

        // PDF MediaBox is an array of four numbers: [LLx, LLy, URx, URy]
        let bounds = media_box_obj.as_array_of::<u32, 4>()?;

        let left = bounds[0];
        let bottom = bounds[1];
        let right = bounds[2];
        let top = bounds[3];

        Ok(Some(MediaBox::new(left, top, right, bottom)))
    }
}
