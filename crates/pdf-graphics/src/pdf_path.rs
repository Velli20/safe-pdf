use crate::transform::Transform;

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
#[derive(Default, Clone)]
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
    pub fn move_to(&mut self, x: f32, y: f32) {
        self.current_x = x;
        self.current_y = y;
        self.verbs.push(PathVerb::MoveTo { x, y });
    }

    /// Appends a `LineTo` verb to the path, updating the current point.
    ///
    /// # Arguments
    ///
    /// - `x`, `y`: The coordinates to draw a line to from the current point.
    pub fn line_to(&mut self, x: f32, y: f32) {
        self.current_x = x;
        self.current_y = y;
        self.verbs.push(PathVerb::LineTo { x, y });
    }

    /// Appends a relative `MoveTo` by offsetting from the current point.
    ///
    /// If the path has no current point yet, the origin (0, 0) is used.
    ///
    /// # Arguments
    ///
    /// - `dx`, `dy`: The offsets to add to the current point.
    pub fn move_rel(&mut self, dx: f32, dy: f32) {
        let x = self.current_x + dx;
        let y = self.current_y + dy;
        self.move_to(x, y);
    }

    /// Appends a relative `LineTo` by offsetting from the current point.
    ///
    /// If the path has no current point yet, the origin (0, 0) is used.
    ///
    /// # Arguments
    ///
    /// - `dx`, `dy`: The offsets to add to the current point.
    pub fn line_rel(&mut self, dx: f32, dy: f32) {
        let x = self.current_x + dx;
        let y = self.current_y + dy;
        self.line_to(x, y);
    }

    /// Appends a `CubicTo` verb to the path, updating the current point to (`x3`, `y3`).
    ///
    /// # Arguments
    ///
    /// - `x1`, `y1`: Coordinates of the first Bézier control point.
    /// - `x2`, `y2`: Coordinates of the second Bézier control point.
    /// - `x3`, `y3`: Coordinates of the end point of the curve.
    pub fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) {
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
    }

    /// Appends a relative cubic Bézier curve (`CubicTo`) updating the current point.
    ///
    /// Control points and the end point are specified relative to the current point,
    /// and accumulate as in PostScript/CFF `rrcurveto`:
    ///
    /// - P1 = P0 + (dx1, dy1)
    /// - P2 = P1 + (dx2, dy2)
    /// - P3 = P2 + (dx3, dy3)
    pub fn curve_rel(&mut self, dx1: f32, dy1: f32, dx2: f32, dy2: f32, dx3: f32, dy3: f32) {
        let x0 = self.current_x;
        let y0 = self.current_y;

        let x1 = x0 + dx1;
        let y1 = y0 + dy1;
        let x2 = x1 + dx2;
        let y2 = y1 + dy2;
        let x3 = x2 + dx3;
        let y3 = y2 + dy3;

        self.curve_to(x1, y1, x2, y2, x3, y3);
    }

    /// Appends a `QuadTo` verb to the path, updating the current point to (`x2`, `y2`).
    ///
    /// # Arguments
    ///
    /// - `x1`, `y1`: Coordinates of the Bézier control point.
    /// - `x2`, `y2`: Coordinates of the end point of the curve.
    pub fn quad_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.current_x = x2;
        self.current_y = y2;

        self.verbs.push(PathVerb::QuadTo { x1, y1, x2, y2 });
    }

    /// Appends a relative quadratic Bézier curve (`QuadTo`) updating the current point.
    ///
    /// Control point and end point are specified relative to the current point:
    /// - Q1 = P0 + (dx1, dy1)
    /// - Q2 = Q1 + (dx2, dy2)
    pub fn quad_rel(&mut self, dx1: f32, dy1: f32, dx2: f32, dy2: f32) {
        let x0 = self.current_x;
        let y0 = self.current_y;

        let x1 = x0 + dx1;
        let y1 = y0 + dy1;
        let x2 = x1 + dx2;
        let y2 = y1 + dy2;

        self.quad_to(x1, y1, x2, y2);
    }

    /// Appends a `Close` verb to the path.
    pub fn close(&mut self) {
        self.verbs.push(PathVerb::Close);
    }

    pub fn transform(&mut self, transform: &Transform) {
        for verb in &mut self.verbs {
            match verb {
                PathVerb::MoveTo { x, y } => {
                    let (xt, yt) = transform.transform_point(*x, *y);
                    *x = xt;
                    *y = yt;
                }
                PathVerb::LineTo { x, y } => {
                    let (xt, yt) = transform.transform_point(*x, *y);
                    *x = xt;
                    *y = yt;
                }
                PathVerb::CubicTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x3,
                    y3,
                } => {
                    let (x1t, y1t) = transform.transform_point(*x1, *y1);
                    let (x2t, y2t) = transform.transform_point(*x2, *y2);
                    let (x3t, y3t) = transform.transform_point(*x3, *y3);
                    *x1 = x1t;
                    *y1 = y1t;
                    *x2 = x2t;
                    *y2 = y2t;
                    *x3 = x3t;
                    *y3 = y3t;
                }
                PathVerb::QuadTo { x1, y1, x2, y2 } => {
                    let (x1t, y1t) = transform.transform_point(*x1, *y1);
                    let (x2t, y2t) = transform.transform_point(*x2, *y2);
                    *x1 = x1t;
                    *y1 = y1t;
                    *x2 = x2t;
                    *y2 = y2t;
                }
                PathVerb::Close => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PathVerb, PdfPath};

    #[test]
    fn test_move_rel_from_origin() {
        let mut p = PdfPath::default();
        assert!(p.current_point().is_none());
        p.move_rel(10.0, 5.0);
        assert_eq!(p.current_point(), Some((10.0, 5.0)));
        assert!(matches!(p.verbs[0], PathVerb::MoveTo { x: 10.0, y: 5.0 }));
    }

    #[test]
    fn test_line_rel() {
        let mut p = PdfPath::default();
        p.move_to(2.0, 3.0);
        p.line_rel(3.0, -2.0);
        assert_eq!(p.current_point(), Some((5.0, 1.0)));
        assert!(matches!(p.verbs[1], PathVerb::LineTo { x: 5.0, y: 1.0 }));
    }

    #[test]
    fn test_curve_rel() {
        let mut p = PdfPath::default();
        p.move_to(0.0, 0.0);
        p.curve_rel(10.0, 0.0, 0.0, 10.0, -5.0, -5.0);
        // P1 = (10,0), P2 = (10,10), P3 = (5,5)
        assert_eq!(p.current_point(), Some((5.0, 5.0)));
        match p.verbs[1] {
            PathVerb::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x3,
                y3,
            } => {
                assert_eq!((x1, y1, x2, y2, x3, y3), (10.0, 0.0, 10.0, 10.0, 5.0, 5.0));
            }
            _ => panic!("expected CubicTo"),
        }
    }

    #[test]
    fn test_quad_rel() {
        let mut p = PdfPath::default();
        p.move_to(1.0, 1.0);
        p.quad_rel(2.0, 0.0, 3.0, 4.0);
        // Q1 = (3,1), Q2 = (6,5)
        assert_eq!(p.current_point(), Some((6.0, 5.0)));
        match p.verbs[1] {
            PathVerb::QuadTo { x1, y1, x2, y2 } => {
                assert_eq!((x1, y1, x2, y2), (3.0, 1.0, 6.0, 5.0));
            }
            _ => panic!("expected QuadTo"),
        }
    }
}
