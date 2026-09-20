//! Browser geometry uses the same device-space path contract as Skia.
use crate::error::WebResult;
use pdf_canvas::CanvasPath;
use pdf_graphics::pdf_path::PathVerb;

/// Converts resolved device-coordinate verbs into browser geometry.
pub(crate) fn prepare(path: &CanvasPath<'_>) -> WebResult<web_sys::Path2d> {
    let result = web_sys::Path2d::new()?;
    for verb in path.verbs() {
        match verb? {
            PathVerb::MoveTo { x, y } => {
                result.move_to(f64::from(x), f64::from(y));
            }
            PathVerb::LineTo { x, y } => {
                result.line_to(f64::from(x), f64::from(y));
            }
            PathVerb::QuadTo { x1, y1, x2, y2 } => {
                result.quadratic_curve_to(
                    f64::from(x1),
                    f64::from(y1),
                    f64::from(x2),
                    f64::from(y2),
                );
            }
            PathVerb::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x3,
                y3,
            } => {
                result.bezier_curve_to(
                    f64::from(x1),
                    f64::from(y1),
                    f64::from(x2),
                    f64::from(y2),
                    f64::from(x3),
                    f64::from(y3),
                );
            }
            PathVerb::Close => result.close_path(),
        }
    }
    Ok(result)
}
