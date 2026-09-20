//! Portable path geometry with deferred mapping into device coordinates.
use std::sync::Arc;

use pdf_graphics::{
    pdf_path::{PathVerb, PdfPath},
    point::Point,
    transform::{Transform, TransformError},
};

#[derive(Clone)]
enum Geometry<'a> {
    Borrowed(&'a PdfPath),
    Owned(PdfPath),
    Shared(Arc<PdfPath>),
}

/// Immutable geometry mapped into logical device coordinates on demand.
///
/// The mapping affects geometry only, not stroke widths, dash lengths, or shaders.
/// Backend viewport scaling is applied separately. Shared paths retain their source
/// geometry when cloned or recorded, without allocating transformed PDF outlines.
#[derive(Clone)]
pub struct CanvasPath<'a> {
    geometry: Geometry<'a>,
    transform: Option<Transform>,
}

impl<'a> CanvasPath<'a> {
    /// Borrows a path that already uses logical device coordinates.
    pub fn device(path: &'a PdfPath) -> Self {
        Self {
            geometry: Geometry::Borrowed(path),
            transform: None,
        }
    }

    /// Borrows geometry with a source-to-device mapping, rejecting nonfinite matrices.
    /// Coordinates are validated when the verbs are consumed.
    pub fn transformed(path: &'a PdfPath, transform: Transform) -> Result<Self, TransformError> {
        transform.validate()?;
        Ok(Self {
            geometry: Geometry::Borrowed(path),
            transform: Some(transform),
        })
    }

    /// Owns the geometry needed to retain this path beyond its source borrow.
    /// Shared geometry keeps its original allocation and deferred mapping.
    pub fn into_owned(self) -> CanvasPath<'static> {
        CanvasPath {
            geometry: match self.geometry {
                Geometry::Borrowed(path) => Geometry::Owned(path.clone()),
                Geometry::Owned(path) => Geometry::Owned(path),
                Geometry::Shared(path) => Geometry::Shared(path),
            },
            transform: self.transform,
        }
    }

    /// Returns the retained source geometry without applying its mapping.
    pub(crate) fn source(&self) -> &PdfPath {
        match &self.geometry {
            Geometry::Borrowed(path) => path,
            Geometry::Owned(path) => path,
            Geometry::Shared(path) => path,
        }
    }

    /// Iterates over device-coordinate verbs without allocating an intermediate path.
    ///
    /// Returns an error for nonfinite source coordinates or mapping overflow.
    /// Finite singular transforms are allowed because no inverse is required.
    pub fn verbs(&self) -> impl Iterator<Item = Result<PathVerb, TransformError>> + '_ {
        self.source().verbs.iter().map(|verb| {
            let point = |x: f32, y: f32| {
                let point = Point::new(x, y);
                match &self.transform {
                    Some(transform) => transform.try_map_point(point),
                    None if x.is_finite() && y.is_finite() => Ok(point),
                    None => Err(TransformError::NonFinite),
                }
            };
            Ok(match *verb {
                PathVerb::MoveTo { x, y } => {
                    let p = point(x, y)?;
                    PathVerb::MoveTo { x: p.x, y: p.y }
                }
                PathVerb::LineTo { x, y } => {
                    let p = point(x, y)?;
                    PathVerb::LineTo { x: p.x, y: p.y }
                }
                PathVerb::QuadTo { x1, y1, x2, y2 } => {
                    let a = point(x1, y1)?;
                    let b = point(x2, y2)?;
                    PathVerb::QuadTo {
                        x1: a.x,
                        y1: a.y,
                        x2: b.x,
                        y2: b.y,
                    }
                }
                PathVerb::CubicTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x3,
                    y3,
                } => {
                    let a = point(x1, y1)?;
                    let b = point(x2, y2)?;
                    let c = point(x3, y3)?;
                    PathVerb::CubicTo {
                        x1: a.x,
                        y1: a.y,
                        x2: b.x,
                        y2: b.y,
                        x3: c.x,
                        y3: c.y,
                    }
                }
                PathVerb::Close => PathVerb::Close,
            })
        })
    }

    /// Materializes a mutable device-space path, validating all mapped coordinates.
    pub fn to_pdf_path(&self) -> Result<PdfPath, TransformError> {
        let mut path = PdfPath::default();
        for verb in self.verbs() {
            match verb? {
                PathVerb::MoveTo { x, y } => path.move_to(x, y),
                PathVerb::LineTo { x, y } => path.line_to(x, y),
                PathVerb::QuadTo { x1, y1, x2, y2 } => path.quad_to(x1, y1, x2, y2),
                PathVerb::CubicTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x3,
                    y3,
                } => path.curve_to(x1, y1, x2, y2, x3, y3),
                PathVerb::Close => path.close(),
            }
        }
        Ok(path)
    }
}

impl CanvasPath<'static> {
    /// Retains shared geometry with a source-to-device mapping.
    /// Rejects nonfinite matrices; coordinates are validated when verbs are consumed.
    pub fn shared(path: Arc<PdfPath>, transform: Transform) -> Result<Self, TransformError> {
        transform.validate()?;
        Ok(Self {
            geometry: Geometry::Shared(path),
            transform: Some(transform),
        })
    }
}
