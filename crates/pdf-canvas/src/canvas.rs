use pdf_graphics::transform::Transform;

use crate::{PathFillType, error::PdfCanvasError, pdf_path::PdfPath};

/// A trait for high-level canvas operations, providing an interface for managing
/// graphics state and transformations.
///
/// This is used internally by this crate to interact with a generic
/// canvas that can save/restore its state and manipulate its transformation matrix.
pub(crate) trait Canvas {
    /// Saves the entire current graphics state onto a stack.
    ///
    /// This includes the current transformation matrix, colors, line styles, and clipping path.
    /// A corresponding call to `restore` is required to pop the state from the stack.
    fn save(&mut self) -> Result<(), PdfCanvasError>;

    /// Restores the most recently saved graphics state from the stack.
    ///
    /// If the restored state included a clipping path, the clipping path is reset on the backend.
    fn restore(&mut self) -> Result<(), PdfCanvasError>;

    /// Replaces the current transformation matrix (CTM) with the given matrix.
    ///
    /// This sets the complete transformation from user space to device space.
    fn set_matrix(&mut self, matrix: Transform) -> Result<(), PdfCanvasError>;

    /// Fills the given path with using a currently set color.
    ///
    /// # Parameters
    ///
    /// - `path`: The path to fill. The coordinates are in the backend's device space.
    /// - `fill_type`: The rule (winding or even-odd) to determine what is "inside" the path.
    fn fill_path(&mut self, path: &PdfPath, fill_type: PathFillType) -> Result<(), PdfCanvasError>;
}
