//! Evaluation, tessellation, and rasterization of mesh shadings.

use std::ops::Range;

use num_traits::ToPrimitive;
use pdf_graphics::{
    Bounds, BoundsAccumulator,
    bezier::{bernstein_basis, evaluate_cubic_bezier},
    color::Color,
    point::Point,
    rect::Rect,
    size::Size,
    transform::Transform,
};
use pdf_image::ImageRasterError;

use crate::{
    model::{MeshPatch, MeshTriangle, MeshVertex},
    paint::ShadingPaint,
    pixel_grid::{PIXEL_CENTER, PixelGrid},
};

/// Fewest and most cells a patch is divided into along each side.
const MIN_SUBDIVISION: u16 = 8;
const MAX_SUBDIVISION: u16 = 32;

/// Largest raster edge a mesh is sampled into.
const MAX_RASTER_DIMENSION: u16 = 2048;

/// How far outside a triangle a pixel center may fall and still be painted, as a
/// barycentric weight, so rounding leaves no gaps between adjacent triangles.
const EDGE_TOLERANCE: f32 = 0.001;

/// A piece of a mesh that paints itself independently of the others.
pub(crate) trait MeshElement: Bounds {
    /// Returns this element with its points mapped by `transform`.
    fn mapped(&self, transform: &Transform) -> Self;

    /// Paints this element, whose points are in pixel coordinates.
    fn fill(&self, raster: &mut MeshRaster);
}

/// Builds a raster paint of `elements` over `bbox` in device space, or over the bounds of
/// the elements without a usable `bbox`.
///
/// A mesh that covers no area paints nothing.
pub(crate) fn paint_mesh<E: MeshElement>(
    elements: &[E],
    bbox: Option<Rect>,
    transform: &Transform,
) -> Result<ShadingPaint, ImageRasterError> {
    let area = bbox
        .map(|bbox| transform.map_rect(&bbox))
        .filter(Rect::<f32>::is_valid)
        .or_else(|| {
            elements
                .iter()
                .filter_map(|element| element.mapped(transform).bounds())
                .reduce(|a, b| a.union(&b))
        })
        .filter(Rect::<f32>::is_valid);
    let Some(area) = area else {
        return Ok(ShadingPaint::transparent());
    };

    let grid = PixelGrid::new(area, Size::new(1.0, 1.0), MAX_RASTER_DIMENSION);
    let placement = grid.placement();
    let to_pixels = placement.try_inverse()?.post_concatenated(transform);
    let mut raster = MeshRaster::new(grid.size());
    for element in elements {
        element.mapped(&to_pixels).fill(&mut raster);
    }
    ShadingPaint::raster_image(grid.image(raster.pixels, false), placement)
}

impl Bounds for MeshTriangle {
    fn bounds(&self) -> Option<Rect> {
        let points = self.vertices.iter().map(|vertex| vertex.point);
        points.collect::<BoundsAccumulator>().finish()
    }
}

impl MeshElement for MeshTriangle {
    fn mapped(&self, transform: &Transform) -> Self {
        Self {
            vertices: self.vertices.map(|vertex| MeshVertex {
                point: transform.map_point(vertex.point),
                color: vertex.color,
            }),
        }
    }

    fn fill(&self, raster: &mut MeshRaster) {
        raster.fill_triangle(self);
    }
}

impl Bounds for MeshPatch {
    fn bounds(&self) -> Option<Rect> {
        let points = self.control_points().iter().copied();
        points.collect::<BoundsAccumulator>().finish()
    }
}

impl MeshElement for MeshPatch {
    fn mapped(&self, transform: &Transform) -> Self {
        match *self {
            Self::Coons {
                control_points,
                corner_colors,
            } => Self::Coons {
                control_points: control_points.map(|point| transform.map_point(point)),
                corner_colors,
            },
            Self::Tensor {
                control_points,
                corner_colors,
            } => Self::Tensor {
                control_points: control_points.map(|point| transform.map_point(point)),
                corner_colors,
            },
        }
    }

    fn fill(&self, raster: &mut MeshRaster) {
        let Some(bounds) = self.bounds() else {
            return;
        };
        let extent = bounds.width().max(bounds.height());
        let steps = (MIN_SUBDIVISION..MAX_SUBDIVISION)
            .find(|steps| f32::from(*steps) >= extent)
            .unwrap_or(MAX_SUBDIVISION);

        let mut upper = self.row(0, steps);
        for row in 1..=steps {
            let lower = self.row(row, steps);
            for (&[top_left, top_right], &[bottom_left, bottom_right]) in
                upper.array_windows().zip(lower.array_windows())
            {
                for vertices in [
                    [top_left, top_right, bottom_left],
                    [top_right, bottom_right, bottom_left],
                ] {
                    raster.fill_triangle(&MeshTriangle { vertices });
                }
            }
            upper = lower;
        }
    }
}

impl MeshPatch {
    fn control_points(&self) -> &[Point] {
        match self {
            Self::Coons { control_points, .. } => control_points,
            Self::Tensor { control_points, .. } => control_points,
        }
    }

    /// Evaluates the vertices of one row of a patch divided into `steps` cells per side.
    fn row(&self, row: u16, steps: u16) -> Vec<MeshVertex> {
        let parameter = |index: u16| f32::from(index) / f32::from(steps);
        (0..=steps)
            .map(|column| self.vertex(parameter(column), parameter(row)))
            .collect()
    }

    /// Evaluates the patch at the `(u, v)` parametric coordinate.
    fn vertex(&self, u: f32, v: f32) -> MeshVertex {
        // Weights of the corners in the clockwise order of the corner colors.
        let corner_weights = [(1.0 - u) * (1.0 - v), u * (1.0 - v), u * v, (1.0 - u) * v];
        let (point, corner_colors) = match self {
            Self::Coons {
                control_points,
                corner_colors,
            } => {
                let &[p0, p1, p2, p3, p4, p5, p6, p7, p8, p9, p10, p11] = control_points;
                // Both ruled surfaces between opposite sides, less the corners they share.
                let top = evaluate_cubic_bezier([p0, p1, p2, p3], u);
                let right = evaluate_cubic_bezier([p3, p4, p5, p6], v);
                let bottom = evaluate_cubic_bezier([p9, p8, p7, p6], u);
                let left = evaluate_cubic_bezier([p0, p11, p10, p9], v);
                let corners = Point::weighted_sum([p0, p3, p6, p9].into_iter().zip(corner_weights));
                let point = Point::weighted_sum([
                    (top, 1.0 - v),
                    (bottom, v),
                    (left, 1.0 - u),
                    (right, u),
                    (corners, -1.0),
                ]);
                (point, corner_colors)
            }
            Self::Tensor {
                control_points,
                corner_colors,
            } => {
                let (rows, _) = control_points.as_chunks::<4>();
                let curve = rows.iter().map(|row| evaluate_cubic_bezier(*row, u));
                let point = Point::weighted_sum(curve.zip(bernstein_basis(v)));
                (point, corner_colors)
            }
        };
        MeshVertex {
            point,
            color: Color::weighted_sum(corner_colors.iter().copied().zip(corner_weights)),
        }
    }
}

/// RGBA8 pixels that mesh triangles are painted into, transparent where none was.
pub(crate) struct MeshRaster {
    size: Size<u16>,
    pixels: Vec<[u8; 4]>,
}

impl MeshRaster {
    fn new(size: Size<u16>) -> Self {
        let pixels = usize::from(size.width).saturating_mul(usize::from(size.height));
        Self {
            size,
            pixels: vec![[0; 4]; pixels],
        }
    }

    /// Paints every pixel whose center lies in `triangle`, given in pixel coordinates.
    fn fill_triangle(&mut self, triangle: &MeshTriangle) {
        let [a, b, c] = triangle.vertices;
        // Maps an offset from `c` to the barycentric weights of `a` and `b`.
        let weights = Transform::from_row(
            a.point.x - c.point.x,
            a.point.y - c.point.y,
            b.point.x - c.point.x,
            b.point.y - c.point.y,
            0.0,
            0.0,
        );
        let (Ok(weights), Some((columns, rows))) = (
            weights.try_inverse(),
            triangle.bounds().and_then(|bounds| self.covered(&bounds)),
        ) else {
            return;
        };

        let lines = self.pixels.chunks_exact_mut(usize::from(self.size.width));
        for (line, row) in lines.skip(usize::from(rows.start)).zip(rows) {
            let y = f32::from(row) + PIXEL_CENTER - c.point.y;
            let pixels = line.iter_mut().skip(usize::from(columns.start));
            for (pixel, column) in pixels.zip(columns.clone()) {
                let x = f32::from(column) + PIXEL_CENTER - c.point.x;
                let Point { x: wa, y: wb } = weights.map_point(Point::new(x, y));
                let wc = 1.0 - wa - wb;
                if wa.min(wb).min(wc) >= -EDGE_TOLERANCE {
                    *pixel = Color::weighted_sum([(a.color, wa), (b.color, wb), (c.color, wc)])
                        .to_rgba8();
                }
            }
        }
    }

    /// Returns the columns and rows of the pixels that `bounds` touches.
    fn covered(&self, bounds: &Rect) -> Option<(Range<u16>, Range<u16>)> {
        let span = |min: f32, max: f32, limit: u16| {
            let pixel = |edge: f32| edge.clamp(0.0, f32::from(limit)).to_u16();
            Some(pixel(min.floor())?..pixel(max.ceil())?)
        };
        Some((
            span(bounds.left, bounds.right, self.size.width)?,
            span(bounds.top, bounds.bottom, self.size.height)?,
        ))
    }
}
