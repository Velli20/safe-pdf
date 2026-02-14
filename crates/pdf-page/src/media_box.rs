use pdf_object::error::ObjectError;
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

/// Defines the page boundaries within a PDF document.
///
/// The `MediaBox` is a rectangle, expressed in default user space units,
/// that defines the boundaries of the physical medium on which the page
/// is intended to be displayed or printed.
#[derive(Default, Debug, Clone)]
pub struct MediaBox {
    /// The x-coordinate of the lower-left corner of the rectangle.
    pub left: f32,
    /// The y-coordinate of the upper-right corner of the rectangle.
    pub top: f32,
    /// The x-coordinate of the upper-right corner of the rectangle.
    pub right: f32,
    /// The y-coordinate of the lower-left corner of the rectangle.
    pub bottom: f32,
}

impl MediaBox {
    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    pub fn height(&self) -> f32 {
        self.top - self.bottom
    }
}

impl MediaBox {
    const KEY: &'static str = "MediaBox";

    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<MediaBox>, ObjectError> {
        let Some(media_box_obj) = dictionary.get(Self::KEY) else {
            return Ok(None);
        };

        // PDF MediaBox is an array of four numbers: [LLx, LLy, URx, URy]
        let [left, bottom, right, top] = media_box_obj.try_array_of::<f32, 4>(objects)?;

        Ok(Some(MediaBox {
            left,
            top,
            right,
            bottom,
        }))
    }
}
