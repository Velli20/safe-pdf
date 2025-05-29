use crate::error::PdfCanvasError;

/// Represents a single operation in a graphics path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathVerb {
    /// Moves the current point to (`x`, `y`) without drawing a line.
    MoveTo { x: f32, y: f32 },
    /// Draws a straight line from the current point to (`x`, `y`).
    LineTo { x: f32, y: f32 },
    /// Draws a cubic Bezier curve from the current point to (`x3`, `y3`).
    /// (`x1`, `y1`) and (`x2`, `y2`) are the control points.
    CubicTo {
        /// The x-coordinate of the first control point.
        x1: f32,
        /// The y-coordinate of the first control point.
        y1: f32,
        /// The x-coordinate of the second control point.
        x2: f32,
        /// The y-coordinate of the second control point.
        y2: f32,
        /// The x-coordinate of the final point of the curve.
        x3: f32,
        /// The y-coordinate of the final point of the curve.
        y3: f32,
    },
    /// Draws a quadratic Bezier curve from the current point to (`x2`, `y2`).
    /// (`x1`, `y1`) is the control point.
    QuadTo {
        /// The x-coordinate of the control point.
        x1: f32,
        /// The y-coordinate of the control point.
        y1: f32,
        /// The x-coordinate of the final point of the curve.
        x2: f32,
        /// The y-coordinate of the final point of the curve.
        y2: f32,
    },
    /// Closes the current subpath by drawing a straight line from the current
    /// point to the starting point of the subpath.
    Close,
}

/// Represents a sequence of path construction operations.
#[derive(Default)]
pub struct PdfPath {
    current_x: f32,
    current_y: f32,
    pub verbs: Vec<PathVerb>,
}

impl PdfPath {
    /// Returns the current point of the path.
    ///
    /// Returns `None` if the path is empty (i.e., no `MoveTo` has been called yet).
    pub fn current_point(&self) -> Option<(f32, f32)> {
        if self.verbs.is_empty() {
            None
        } else {
            Some((self.current_x, self.current_y))
        }
    }

    /// Appends a `MoveTo` verb to the path, updating the current point.
    ///
    /// # Arguments
    ///
    /// - `x`, `y`: The coordinates to move to.
    pub fn move_to(&mut self, x: f32, y: f32) -> Result<(), PdfCanvasError> {
        self.current_x = x;
        self.current_y = y;
        self.verbs.push(PathVerb::MoveTo { x, y });

        Ok(())
    }

    /// Appends a `LineTo` verb to the path, updating the current point.
    ///
    /// # Arguments
    ///
    /// - `x`, `y`: The coordinates to draw a line to from the current point.
    pub fn line_to(&mut self, x: f32, y: f32) -> Result<(), PdfCanvasError> {
        self.current_x = x;
        self.current_y = y;
        self.verbs.push(PathVerb::LineTo { x, y });
        Ok(())
    }

    /// Appends a `CubicTo` verb to the path, updating the current point to (`x3`, `y3`).
    ///
    /// # Arguments
    ///
    /// - `x1`, `y1`: Coordinates of the first Bézier control point.
    /// - `x2`, `y2`: Coordinates of the second Bézier control point.
    /// - `x3`, `y3`: Coordinates of the end point of the curve.
    pub fn curve_to(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
    ) -> Result<(), PdfCanvasError> {
        self.current_x = x3;
        self.current_y = y3;

        self.verbs.push(PathVerb::CubicTo {
            x1,
            y1,
            x2,
            y2,
            x3,
            y3,
        });

        Ok(())
    }

    /// Appends a `QuadTo` verb to the path, updating the current point to (`x2`, `y2`).
    ///
    /// # Arguments
    ///
    /// - `x1`, `y1`: Coordinates of the Bézier control point.
    /// - `x2`, `y2`: Coordinates of the end point of the curve.
    pub fn quad_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) -> Result<(), PdfCanvasError> {
        self.current_x = x2;
        self.current_y = y2;

        self.verbs.push(PathVerb::QuadTo { x1, y1, x2, y2 });

        Ok(())
    }

    /// Appends a `Close` verb to the path.
    pub fn close(&mut self) -> Result<(), PdfCanvasError> {
        self.verbs.push(PathVerb::Close);
        Ok(())
    }
}
