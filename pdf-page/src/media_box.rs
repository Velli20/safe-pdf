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
}
