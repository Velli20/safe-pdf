//! Defines traits for processing categorized PDF content stream operators.
//! Implementors of these traits can define how to handle specific groups of
//! PDF drawing, text, and state commands, allowing for different backends
//! (e.g., renderers, text extractors) to selectively implement functionality.

use std::rc::Rc;

use pdf_object::dictionary::Dictionary;

use crate::TextElement;

pub trait PdfOperatorBackendError {
    /// The error type that can be returned by operator handling methods.
    type ErrorType;
}

/// Defines methods for handling PDF path construction operators.
///
/// These operators are used to define shapes and paths before they are painted.
pub trait PathConstructionOps: PdfOperatorBackendError {
    /// Moves the current point to the specified coordinates (x, y), starting a new subpath.
    ///
    /// # Parameters
    ///
    /// - `x`: The x-coordinate of the new current point.
    /// - `y`: The y-coordinate of the new current point.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn move_to(&mut self, x: f32, y: f32) -> Result<(), Self::ErrorType>;

    /// Appends a straight line segment from the current point to (x, y).
    /// The new current point becomes (x, y).
    ///
    /// # Parameters
    ///
    /// - `x`: The x-coordinate of the line's end point.
    /// - `y`: The y-coordinate of the line's end point.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn line_to(&mut self, x: f32, y: f32) -> Result<(), Self::ErrorType>;

    /// Appends a cubic Bézier curve to the current path.
    /// The curve extends from the current point to (x3, y3), using (x1, y1) and (x2, y2) as control points.
    /// The new current point becomes (x3, y3).
    ///
    /// # Parameters
    ///
    /// - `x1`: The x-coordinate of the first control point.
    /// - `y1`: The y-coordinate of the first control point.
    /// - `x2`: The x-coordinate of the second control point.
    /// - `y2`: The y-coordinate of the second control point.
    /// - `x3`: The x-coordinate of the curve's end point.
    /// - `y3`: The y-coordinate of the curve's end point.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn curve_to(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
    ) -> Result<(), Self::ErrorType>;

    /// Appends a cubic Bézier curve to the current path, where the current point is the first control point.
    /// (x2, y2) is the second control point, and (x3, y3) is the end point.
    /// The new current point becomes (x3, y3).
    ///
    /// # Parameters
    ///
    /// - `x2`: The x-coordinate of the second control point.
    /// - `y2`: The y-coordinate of the second control point.
    /// - `x3`: The x-coordinate of the curve's end point.
    /// - `y3`: The y-coordinate of the curve's end point.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn curve_to_v(&mut self, x2: f32, y2: f32, x3: f32, y3: f32) -> Result<(), Self::ErrorType>;

    /// Appends a cubic Bézier curve to the current path, where the endpoint (x3, y3) is also the second control point.
    /// (x1, y1) is the first control point.
    /// The new current point becomes (x3, y3).
    ///
    /// # Parameters
    ///
    /// - `x1`: The x-coordinate of the first control point.
    /// - `y1`: The y-coordinate of the first control point.
    /// - `x3`: The x-coordinate of the curve's end point (and second control point).
    /// - `y3`: The y-coordinate of the curve's end point (and second control point).
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn curve_to_y(&mut self, x1: f32, y1: f32, x3: f32, y3: f32) -> Result<(), Self::ErrorType>;

    /// Closes the current subpath by appending a line segment from the current point to the subpath's starting point.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn close_path(&mut self) -> Result<(), Self::ErrorType>;

    /// Appends a rectangle to the current path as a complete subpath.
    /// The rectangle is defined by its bottom-left corner (x, y), width, and height.
    ///
    /// # Parameters
    ///
    /// - `x`: The x-coordinate of the rectangle's bottom-left corner.
    /// - `y`: The y-coordinate of the rectangle's bottom-left corner.
    /// - `width`: The width of the rectangle.
    /// - `height`: The height of the rectangle.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn rectangle(&mut self, x: f32, y: f32, width: f32, height: f32)
    -> Result<(), Self::ErrorType>;
}

/// Defines methods to handle PDF Path Painting operators.
pub trait PathPaintingOps: PdfOperatorBackendError {
    /// Strokes the current path using the current color and line style.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn stroke_path(&mut self) -> Result<(), Self::ErrorType>;

    /// Closes the current subpath and then strokes it.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn close_and_stroke_path(&mut self) -> Result<(), Self::ErrorType>;

    /// Fills the current path using the non-zero winding number rule.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn fill_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType>;

    /// Fills the current path using the even-odd rule.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn fill_path_even_odd(&mut self) -> Result<(), Self::ErrorType>;

    /// Fills and then strokes the current path, using the non-zero winding number rule for filling.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn fill_and_stroke_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType>;

    /// Fills and then strokes the current path, using the even-odd rule for filling.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn fill_and_stroke_path_even_odd(&mut self) -> Result<(), Self::ErrorType>;

    /// Closes, fills, and then strokes the current path, using the non-zero winding number rule for filling.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn close_fill_and_stroke_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType>;

    /// Closes, fills, and then strokes the current path, using the even-odd rule for filling.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn close_fill_and_stroke_path_even_odd(&mut self) -> Result<(), Self::ErrorType>;

    /// Ends the current path without filling or stroking it, effectively discarding it.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn end_path_no_op(&mut self) -> Result<(), Self::ErrorType>;
}

/// Defines methods to handle PDF Clipping Path operators.
pub trait ClippingPathOps: PdfOperatorBackendError {
    /// Modifies the current clipping path by intersecting it with the current path,
    /// using the non-zero winding number rule.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn clip_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType>;

    /// Modifies the current clipping path by intersecting it with the current path,
    /// using the even-odd rule.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn clip_path_even_odd(&mut self) -> Result<(), Self::ErrorType>;
}

/// Defines methods to handle PDF Graphics State operators.
pub trait GraphicsStateOps: PdfOperatorBackendError {
    /// Saves the current graphics state onto the graphics state stack.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn save_graphics_state(&mut self) -> Result<(), Self::ErrorType>;

    /// Restores the graphics state by removing the most recently saved state from the stack.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn restore_graphics_state(&mut self) -> Result<(), Self::ErrorType>;

    /// Modifies the current transformation matrix (CTM) by concatenating it with the specified matrix.
    /// The matrix is `[a b c d e f]`.
    ///
    /// # Parameters
    ///
    /// - `a`: Horizontal scaling.
    /// - `b`: Skewing factor; affects the y-coordinate based on the x-coordinate.
    /// - `c`: Skewing factor; affects the x-coordinate based on the y-coordinate.
    /// - `d`: Vertical scaling.
    /// - `e`: Horizontal translation.
    /// - `f`: Vertical translation.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn concat_matrix(
        &mut self,
        a: f32,
        b: f32,
        c: f32,
        d: f32,
        e: f32,
        f: f32,
    ) -> Result<(), Self::ErrorType>;

    /// Sets the line width for path stroking.
    ///
    /// # Parameters
    ///
    /// - `width`: The new line width in user space units.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_line_width(&mut self, width: f32) -> Result<(), Self::ErrorType>;

    /// Sets the line cap style for path stroking.
    ///
    /// # Parameters
    ///
    /// - `cap_style`: An integer representing the cap style (e.g., 0 for butt, 1 for
    ///    round, 2 for projecting square).
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_line_cap(&mut self, cap_style: i32) -> Result<(), Self::ErrorType>;

    /// Sets the line join style for path stroking.
    ///
    /// # Parameters
    ///
    /// - `join_style`: An integer representing the join style (e.g., 0 for miter, 1 for round, 2 for bevel).
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_line_join(&mut self, join_style: i32) -> Result<(), Self::ErrorType>;

    /// Sets the miter limit for path stroking.
    ///
    /// # Parameters
    ///
    /// - `limit`: The miter limit.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_miter_limit(&mut self, limit: f32) -> Result<(), Self::ErrorType>;

    /// Sets the dash pattern for path stroking.
    ///
    /// # Parameters
    ///
    /// - `dash_array`: A slice representing the lengths of alternating dashes and gaps.
    /// - `dash_phase`: The distance into the dash pattern at which to start.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_dash_pattern(
        &mut self,
        dash_array: &[f32],
        dash_phase: f32,
    ) -> Result<(), Self::ErrorType>;

    /// Sets the rendering intent for color reproduction.
    ///
    /// # Parameters
    ///
    /// - `intent`: A string representing the rendering intent.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_rendering_intent(&mut self, intent: &str) -> Result<(), Self::ErrorType>;

    /// Sets the flatness tolerance, controlling the accuracy of curve rendering.
    ///
    /// # Parameters
    ///
    /// - `tolerance`: The flatness tolerance value.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_flatness_tolerance(&mut self, tolerance: f32) -> Result<(), Self::ErrorType>;

    /// Sets multiple graphics state parameters from a named graphics state parameter dictionary.
    /// The dictionary is expected to be in the resource dictionary.
    ///
    /// # Parameters
    ///
    /// - `dict_name`: The name of the graphics state parameter dictionary.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_graphics_state_from_dict(&mut self, dict_name: &str) -> Result<(), Self::ErrorType>;
}

/// Defines methods to handle PDF Color operators.
pub trait ColorOps: PdfOperatorBackendError {
    /// Sets the color space for subsequent stroking operations.
    ///
    /// # Parameters
    ///
    /// - `name`: The name of the color space (e.g., "DeviceGray", "DeviceRGB", "DeviceCMYK").
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_stroking_color_space(&mut self, name: &str) -> Result<(), Self::ErrorType>;

    /// Sets the color space for subsequent non-stroking (fill) operations.
    ///
    /// # Parameters
    ///
    /// - `name`: The name of the color space (e.g., "DeviceGray", "DeviceRGB", "DeviceCMYK").
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_non_stroking_color_space(&mut self, name: &str) -> Result<(), Self::ErrorType>;

    /// Sets the color for subsequent stroking operations, using the current stroking color space.
    /// The number of components depends on the active color space.
    ///
    /// # Parameters
    ///
    /// - `components`: A slice of color components.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_stroking_color(&mut self, components: &[f32]) -> Result<(), Self::ErrorType>;

    /// Sets the color for stroking operations, potentially with a pattern name.
    /// Used for color spaces like ICCBased, Separation, DeviceN, or Pattern.
    ///
    /// # Parameters
    ///
    /// - `components`: A slice of color components.
    /// - `pattern_name`: An optional name of a pattern, if a Pattern color space is active.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_stroking_color_extended(
        &mut self,
        components: &[f32],
        pattern_name: Option<&str>,
    ) -> Result<(), Self::ErrorType>;

    /// Sets the color for subsequent non-stroking (fill) operations, using the current non-stroking color space.
    /// The number of components depends on the active color space.
    ///
    /// # Parameters
    ///
    /// - `components`: A slice of color components.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_non_stroking_color(&mut self, components: &[f32]) -> Result<(), Self::ErrorType>;

    /// Sets the color for non-stroking (fill) operations, potentially with a pattern name.
    /// Used for color spaces like ICCBased, Separation, DeviceN, or Pattern.
    ///
    /// # Parameters
    ///
    /// - `components`: A slice of color components.
    /// - `pattern_name`: An optional name of a pattern, if a Pattern color space is active.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_non_stroking_color_extended(
        &mut self,
        components: &[f32],
        pattern_name: Option<&str>,
    ) -> Result<(), Self::ErrorType>;

    /// Sets the stroking color to a grayscale value.
    /// Assumes DeviceGray color space or similar.
    ///
    /// # Parameters
    ///
    /// - `gray`: The gray level (0.0 for black, 1.0 for white).
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_stroking_gray(&mut self, gray: f32) -> Result<(), Self::ErrorType>;

    /// Sets the non-stroking (fill) color to a grayscale value.
    /// Assumes DeviceGray color space or similar.
    ///
    /// # Parameters
    ///
    /// - `gray`: The gray level (0.0 for black, 1.0 for white).
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_non_stroking_gray(&mut self, gray: f32) -> Result<(), Self::ErrorType>;

    /// Sets the stroking color to an RGB value.
    /// Assumes DeviceRGB color space or similar.
    ///
    /// # Parameters
    ///
    /// - `r`: Red component (0.0 to 1.0).
    /// - `g`: Green component (0.0 to 1.0).
    /// - `b`: Blue component (0.0 to 1.0).
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_stroking_rgb(&mut self, r: f32, g: f32, b: f32) -> Result<(), Self::ErrorType>;

    /// Sets the non-stroking (fill) color to an RGB value.
    /// Assumes DeviceRGB color space or similar.
    ///
    /// # Parameters
    ///
    /// - `r`: Red component (0.0 to 1.0).
    /// - `g`: Green component (0.0 to 1.0).
    /// - `b`: Blue component (0.0 to 1.0).
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_non_stroking_rgb(&mut self, r: f32, g: f32, b: f32) -> Result<(), Self::ErrorType>;

    /// Sets the stroking color to a CMYK value.
    /// Assumes DeviceCMYK color space or similar.
    ///
    /// # Parameters
    ///
    /// - `c`: Cyan component (0.0 to 1.0).
    /// - `m`: Magenta component (0.0 to 1.0).
    /// - `y`: Yellow component (0.0 to 1.0).
    /// - `k`: Black (Key) component (0.0 to 1.0).
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_stroking_cmyk(&mut self, c: f32, m: f32, y: f32, k: f32) -> Result<(), Self::ErrorType>;

    /// Sets the non-stroking (fill) color to a CMYK value.
    /// Assumes DeviceCMYK color space or similar.
    ///
    /// # Parameters
    ///
    /// - `c`: Cyan component (0.0 to 1.0).
    /// - `m`: Magenta component (0.0 to 1.0).
    /// - `y`: Yellow component (0.0 to 1.0).
    /// - `k`: Black (Key) component (0.0 to 1.0).
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_non_stroking_cmyk(
        &mut self,
        c: f32,
        m: f32,
        y: f32,
        k: f32,
    ) -> Result<(), Self::ErrorType>;
}

/// Defines methods to handle PDF Text Object operators.
pub trait TextObjectOps: PdfOperatorBackendError {
    /// Begins a text object, initializing the text transformation matrices.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn begin_text_object(&mut self) -> Result<(), Self::ErrorType>;

    /// Ends a text object, discarding the text transformation matrices.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn end_text_object(&mut self) -> Result<(), Self::ErrorType>;
}

/// Defines methods to handle PDF Text State operators.
pub trait TextStateOps: PdfOperatorBackendError {
    /// Sets the character spacing.
    ///
    /// # Parameters
    ///
    /// - `spacing`: The character spacing in unscaled text space units.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_character_spacing(&mut self, spacing: f32) -> Result<(), Self::ErrorType>;

    /// Sets the word spacing.
    ///
    /// # Parameters
    ///
    /// - `spacing`: The word spacing in unscaled text space units.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_word_spacing(&mut self, spacing: f32) -> Result<(), Self::ErrorType>;

    /// Sets the horizontal scaling for text.
    ///
    /// # Parameters
    ///
    /// - `scale_percent`: The horizontal scaling factor as a percentage (e.g., 100.0 for 100%).
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_horizontal_text_scaling(&mut self, scale_percent: f32) -> Result<(), Self::ErrorType>;

    /// Sets the text leading (vertical distance between baselines).
    ///
    /// # Parameters
    ///
    /// - `leading`: The text leading in unscaled text space units.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_text_leading(&mut self, leading: f32) -> Result<(), Self::ErrorType>;

    /// Sets the text font and size.
    ///
    /// # Parameters
    ///
    /// - `font_name`: The name of the font resource.
    /// - `size`: The font size in unscaled text space units.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_font_and_size(&mut self, font_name: &str, size: f32) -> Result<(), Self::ErrorType>;

    /// Sets the text rendering mode (e.g., fill, stroke, clip).
    ///
    /// # Parameters
    ///
    /// - `mode`: An integer representing the rendering mode.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_text_rendering_mode(&mut self, mode: i32) -> Result<(), Self::ErrorType>;

    /// Sets the text rise (vertical baseline offset).
    ///
    /// # Parameters
    ///
    /// - `rise`: The text rise in unscaled text space units.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_text_rise(&mut self, rise: f32) -> Result<(), Self::ErrorType>;
}

/// Defines methods to handle PDF Text Positioning operators.
pub trait TextPositioningOps: PdfOperatorBackendError {
    /// Moves the text position to the start of the next line, offset by (tx, ty).
    ///
    /// # Parameters
    ///
    /// - `tx`: The horizontal offset in unscaled text space units.
    /// - `ty`: The vertical offset in unscaled text space units.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn move_text_position(&mut self, tx: f32, ty: f32) -> Result<(), Self::ErrorType>;

    /// Moves the text position and sets the text leading.
    /// The leading is set to -ty.
    ///
    /// # Parameters
    ///
    /// - `tx`: The horizontal offset in unscaled text space units.
    /// - `ty`: The vertical offset in unscaled text space units; leading is set to -ty.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn move_text_position_and_set_leading(
        &mut self,
        tx: f32,
        ty: f32,
    ) -> Result<(), Self::ErrorType>;

    /// Sets the text matrix and text line matrix.
    /// The matrix is `[a b c d e f]`.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_text_matrix(
        &mut self,
        a: f32,
        b: f32,
        c: f32,
        d: f32,
        e: f32,
        f: f32,
    ) -> Result<(), Self::ErrorType>;

    /// Moves to the start of the next text line, using the current text leading.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn move_to_start_of_next_line(&mut self) -> Result<(), Self::ErrorType>;
}

/// Defines methods to handle PDF Text Showing operators.
pub trait TextShowingOps: PdfOperatorBackendError {
    /// Shows a text string at the current text position.
    ///
    /// # Parameters
    ///
    /// - `text`: A byte slice representing the text string, typically encoded according to the current font.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn show_text(&mut self, text: &[u8]) -> Result<(), Self::ErrorType>;

    /// Shows text, allowing individual adjustments to glyph positions.
    ///
    /// # Parameters
    ///
    /// - `elements`: A slice of `TextElement`s, which can be strings or numeric adjustments.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn show_text_with_glyph_positioning(
        &mut self,
        elements: &[TextElement],
    ) -> Result<(), Self::ErrorType>;

    /// Moves to the next line and shows a text string.
    ///
    /// # Parameters
    ///
    /// - `text`: A byte slice representing the text string.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn move_to_next_line_and_show_text(&mut self, text: &[u8]) -> Result<(), Self::ErrorType>;

    /// Sets word and character spacing, moves to the next line, and shows a text string.
    ///
    /// # Parameters
    ///
    /// - `word_spacing`: The word spacing to set.
    /// - `char_spacing`: The character spacing to set.
    /// - `text`: A byte slice representing the text string.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn set_spacing_and_show_text(
        &mut self,
        word_spacing: f32,
        char_spacing: f32,
        text: &[u8],
    ) -> Result<(), Self::ErrorType>;
}

/// Defines methods to handle PDF XObject operators.
pub trait XObjectOps: PdfOperatorBackendError {
    /// Invokes a named external object (XObject), such as an image or a form.
    ///
    /// # Parameters
    ///
    /// - `xobject_name`: The name of the XObject resource.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn invoke_xobject(&mut self, xobject_name: &str) -> Result<(), Self::ErrorType>;
}

/// Defines methods to handle PDF Shading operators.
pub trait ShadingOps: PdfOperatorBackendError {
    /// Paints an area defined by a named shading pattern. (PDF Spec: Section 4.6.3)
    ///
    /// # Parameters
    ///
    /// - `shading_name`: The name of the shading resource.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn paint_shading(&mut self, shading_name: &str) -> Result<(), Self::ErrorType>;
}

/// Defines methods to handle PDF Marked Content operators.
pub trait MarkedContentOps: PdfOperatorBackendError {
    /// Defines a marked-content point, associating it with a tag.
    ///
    /// # Parameters
    ///
    /// - `tag`: The tag for the marked-content point.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn mark_point(&mut self, tag: &str) -> Result<(), Self::ErrorType>;

    /// Defines a marked-content point, associating it with a tag and a property list.
    ///
    /// # Parameters
    ///
    /// - `tag`: The tag for the marked-content point.
    /// - `properties_name_or_dict`: The name of a property list in the resource dictionary or an inline dictionary.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn mark_point_with_properties(
        &mut self,
        tag: &str,
        properties_name_or_dict: &str,
    ) -> Result<(), Self::ErrorType>;

    /// Begins a marked-content sequence with a tag.
    ///
    /// # Parameters
    ///
    /// - `tag`: The tag for the marked-content sequence.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn begin_marked_content(&mut self, tag: &str) -> Result<(), Self::ErrorType>;

    /// Begins a marked-content sequence with a tag and associated properties.
    ///
    /// # Parameters
    ///
    /// - `tag`: The tag for the marked-content sequence.
    /// - `properties_name_or_dict`: The name of a property list in the resource
    /// dictionary or an inline dictionary.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn begin_marked_content_with_properties(
        &mut self,
        tag: &str,
        properties_name_or_dict: &Rc<Dictionary>,
    ) -> Result<(), Self::ErrorType>;

    /// Ends a marked-content sequence.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `ErrorType` on failure.
    fn end_marked_content(&mut self) -> Result<(), Self::ErrorType>;
}

/// A comprehensive backend that implements all operator categories.
/// This can be used as a blanket implementation if a backend supports everything,
/// or as a way to group all the specialized traits.
pub trait PdfOperatorBackend:
    PdfOperatorBackendError
    + PathConstructionOps
    + PathPaintingOps
    + ClippingPathOps
    + GraphicsStateOps
    + ColorOps
    + TextObjectOps
    + TextStateOps
    + TextPositioningOps
    + TextShowingOps
    + XObjectOps
    + ShadingOps
    + MarkedContentOps
{
}
