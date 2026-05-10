//! Mesh shading evaluation, tessellation, and rasterization helpers.

use num_traits::ToPrimitive;
use pdf_graphics::{
    BoundsAccumulator,
    bezier::{bernstein_basis, evaluate_cubic_bezier},
    color::Color,
    point::Point,
    rect::Rect,
    transform::Transform,
};

use crate::model::MeshPatch;

const DEFAULT_SUBDIVISION: usize = 8;
const MAX_SUBDIVISION: f32 = 32.0;
const MAX_RASTER_DIMENSION: usize = 2048;

/// A borrowed mesh patch view used by tessellation and rasterization helpers.
#[derive(Debug, Clone, Copy)]
pub enum MeshPatchRef<'a> {
    /// A borrowed Coons patch.
    Coons {
        /// Boundary control points for the patch.
        control_points: &'a [Point; 12],
        /// Corner colors ordered clockwise from the top-left corner.
        corner_colors: &'a [Color; 4],
    },
    /// A borrowed tensor-product patch.
    Tensor {
        /// Control points for the 4x4 patch net.
        control_points: &'a [Point; 16],
        /// Corner colors ordered clockwise from the top-left corner.
        corner_colors: &'a [Color; 4],
    },
}

/// A tessellated mesh vertex with a device-space position and interpolated color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshVertex {
    /// The device-space point.
    pub point: Point,
    /// The interpolated vertex color.
    pub color: Color,
}

/// A rasterized patch mesh in device space.
#[derive(Debug, Clone, PartialEq)]
pub struct RasterizedPatchMesh {
    /// Packed RGBA8 pixels in row-major order.
    pub pixels: Vec<u8>,
    /// Destination rectangle for the rasterized image.
    pub bounds: Rect,
    /// Raster width in pixels.
    pub width: usize,
    /// Raster height in pixels.
    pub height: usize,
}

impl<'a> MeshPatchRef<'a> {
    /// Creates a borrowed mesh view from a parsed [`MeshPatch`].
    pub fn from_patch(patch: &'a MeshPatch) -> Self {
        match patch {
            MeshPatch::Coons {
                control_points,
                corner_colors,
            } => Self::Coons {
                control_points,
                corner_colors,
            },
            MeshPatch::Tensor {
                control_points,
                corner_colors,
            } => Self::Tensor {
                control_points,
                corner_colors,
            },
        }
    }

    fn control_points(&self) -> &'a [Point] {
        match self {
            Self::Coons { control_points, .. } => control_points.as_slice(),
            Self::Tensor { control_points, .. } => control_points.as_slice(),
        }
    }

    fn evaluate(self, u: f32, v: f32) -> MeshVertex {
        match self {
            Self::Coons {
                control_points,
                corner_colors,
            } => evaluate_coons_patch_vertex(control_points, corner_colors, u, v),
            Self::Tensor {
                control_points,
                corner_colors,
            } => evaluate_tensor_patch_vertex(control_points, corner_colors, u, v),
        }
    }
}

/// Evaluates a Coons patch vertex at the `(u, v)` parametric coordinate.
pub fn evaluate_coons_patch_vertex(
    control_points: &[Point; 12],
    corner_colors: &[Color; 4],
    u: f32,
    v: f32,
) -> MeshVertex {
    let &[p0, p1, p2, p3, p4, p5, p6, p7, p8, p9, p10, p11] = control_points;

    let top = evaluate_cubic_bezier([p0, p1, p2, p3], u);
    let right = evaluate_cubic_bezier([p3, p4, p5, p6], v);
    let bottom = evaluate_cubic_bezier([p9, p8, p7, p6], u);
    let left = evaluate_cubic_bezier([p0, p11, p10, p9], v);
    let bilinear = bilinear_point(p0, p3, p6, p9, u, v);

    MeshVertex {
        point: Point::new(
            (1.0 - v) * top.x + v * bottom.x + (1.0 - u) * left.x + u * right.x - bilinear.x,
            (1.0 - v) * top.y + v * bottom.y + (1.0 - u) * left.y + u * right.y - bilinear.y,
        ),
        color: bilinear_color(corner_colors, u, v),
    }
}

/// Evaluates a tensor-product patch vertex at the `(u, v)` parametric coordinate.
pub fn evaluate_tensor_patch_vertex(
    control_points: &[Point; 16],
    corner_colors: &[Color; 4],
    u: f32,
    v: f32,
) -> MeshVertex {
    let u_basis = bernstein_basis(u);
    let v_basis = bernstein_basis(v);
    let mut x = 0.0;
    let mut y = 0.0;

    for (row, v_weight) in control_points.chunks_exact(4).zip(v_basis) {
        for (&point, u_weight) in row.iter().zip(u_basis) {
            let weight = u_weight * v_weight;
            x += point.x * weight;
            y += point.y * weight;
        }
    }

    MeshVertex {
        point: Point::new(x, y),
        color: bilinear_color(corner_colors, u, v),
    }
}

/// Computes device-space bounds for a sequence of patches after applying `transform`.
pub fn patch_mesh_bounds<'a, I>(patches: I, transform: &Transform) -> Option<Rect>
where
    I: IntoIterator<Item = MeshPatchRef<'a>>,
{
    let mut bounds = BoundsAccumulator::new();

    for patch in patches {
        for point in patch.control_points() {
            bounds.include(transformed_point(*point, transform));
        }
    }

    bounds.finish()
}

/// Chooses a tessellation subdivision count for one patch in device space.
pub fn patch_subdivision(patch: MeshPatchRef<'_>, transform: &Transform) -> usize {
    let Some(bounds) = patch_mesh_bounds(std::iter::once(patch), transform) else {
        return DEFAULT_SUBDIVISION;
    };

    let normalized = bounds.normalized();
    let target = normalized
        .width()
        .max(normalized.height())
        .clamp(8.0, MAX_SUBDIVISION)
        .round();

    match target.to_usize() {
        Some(value) => value,
        None => DEFAULT_SUBDIVISION,
    }
}

/// Tessellates a patch into device-space triangles.
pub fn tessellate_patch(
    patch: MeshPatchRef<'_>,
    transform: &Transform,
    subdivision: usize,
) -> Vec<[MeshVertex; 3]> {
    let steps = subdivision.max(1);
    let rows = build_patch_rows(patch, transform, steps);
    let mut triangles = Vec::with_capacity(steps.saturating_mul(steps).saturating_mul(2));

    for row_pair in rows.windows(2) {
        let [top_row, bottom_row] = row_pair else {
            continue;
        };

        for (top_pair, bottom_pair) in top_row.windows(2).zip(bottom_row.windows(2)) {
            let [top_left, top_right] = top_pair else {
                continue;
            };
            let [bottom_left, bottom_right] = bottom_pair else {
                continue;
            };

            triangles.push([*top_left, *top_right, *bottom_left]);
            triangles.push([*top_right, *bottom_right, *bottom_left]);
        }
    }

    triangles
}

/// Rasterizes a sequence of mesh patches into an RGBA8 image.
pub fn rasterize_patch_mesh<'a, I>(
    patches: I,
    bounds: Rect,
    transform: &Transform,
    max_dimension: usize,
) -> RasterizedPatchMesh
where
    I: IntoIterator<Item = MeshPatchRef<'a>>,
{
    let bounds = bounds.normalized();
    let width = bounded_raster_dimension(bounds.width(), max_dimension);
    let height = bounded_raster_dimension(bounds.height(), max_dimension);
    let mut pixels = vec![255_u8; width.saturating_mul(height).saturating_mul(4)];

    for patch in patches {
        let subdivision = patch_subdivision(patch, transform);
        let triangles = tessellate_patch(patch, transform, subdivision);
        for triangle in triangles {
            rasterize_triangle(&mut pixels, width, height, &bounds, triangle);
        }
    }

    RasterizedPatchMesh {
        pixels,
        bounds,
        width,
        height,
    }
}

/// Rasterizes one triangle into an RGBA8 image buffer.
pub fn rasterize_triangle(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    bounds: &Rect,
    triangle: [MeshVertex; 3],
) {
    let Some(scan_bounds) = triangle_scan_bounds(triangle, bounds, width, height) else {
        return;
    };

    let start_x = scan_bounds.left.max(0.0);
    let end_x = scan_bounds.right;
    let start_y = scan_bounds.top.max(0.0);
    let end_y = scan_bounds.bottom;

    let mut y = start_y;
    while y < end_y {
        let mut x = start_x;
        let sample_y = bounds.top + y + 0.5;
        while x < end_x {
            let sample_x = bounds.left + x + 0.5;
            if let Some((w0, w1, w2)) = barycentric_weights(triangle, sample_x, sample_y) {
                let color = interpolate_triangle_color(triangle, w0, w1, w2);
                write_rgba_pixel(pixels, width, x, y, color);
            }

            x += 1.0;
        }

        y += 1.0;
    }
}

/// Builds a raster image for mesh shading paint with the default size cap.
pub fn rasterize_mesh_patches<'a, I>(
    patches: I,
    bounds: Rect,
    transform: &Transform,
) -> RasterizedPatchMesh
where
    I: IntoIterator<Item = MeshPatchRef<'a>>,
{
    rasterize_patch_mesh(patches, bounds, transform, MAX_RASTER_DIMENSION)
}

fn transformed_point(point: Point, transform: &Transform) -> Point {
    let (x, y) = transform.transform_point(point.x, point.y);
    Point::new(x, y)
}

fn bilinear_point(
    top_left: Point,
    top_right: Point,
    bottom_right: Point,
    bottom_left: Point,
    u: f32,
    v: f32,
) -> Point {
    let one_minus_u = 1.0 - u;
    let one_minus_v = 1.0 - v;

    Point::new(
        one_minus_u * one_minus_v * top_left.x
            + u * one_minus_v * top_right.x
            + u * v * bottom_right.x
            + one_minus_u * v * bottom_left.x,
        one_minus_u * one_minus_v * top_left.y
            + u * one_minus_v * top_right.y
            + u * v * bottom_right.y
            + one_minus_u * v * bottom_left.y,
    )
}

fn bilinear_color(corner_colors: &[Color; 4], u: f32, v: f32) -> Color {
    let [top_left, top_right, bottom_right, bottom_left] = *corner_colors;
    let one_minus_u = 1.0 - u;
    let one_minus_v = 1.0 - v;

    Color::from_rgba(
        one_minus_u * one_minus_v * top_left.r
            + u * one_minus_v * top_right.r
            + u * v * bottom_right.r
            + one_minus_u * v * bottom_left.r,
        one_minus_u * one_minus_v * top_left.g
            + u * one_minus_v * top_right.g
            + u * v * bottom_right.g
            + one_minus_u * v * bottom_left.g,
        one_minus_u * one_minus_v * top_left.b
            + u * one_minus_v * top_right.b
            + u * v * bottom_right.b
            + one_minus_u * v * bottom_left.b,
        one_minus_u * one_minus_v * top_left.a
            + u * one_minus_v * top_right.a
            + u * v * bottom_right.a
            + one_minus_u * v * bottom_left.a,
    )
}

fn build_patch_rows(
    patch: MeshPatchRef<'_>,
    transform: &Transform,
    steps: usize,
) -> Vec<Vec<MeshVertex>> {
    let mut rows = Vec::with_capacity(steps.saturating_add(1));
    let Some(steps_f32) = steps.to_f32() else {
        return rows;
    };

    for row in 0..=steps {
        let Some(row_f32) = row.to_f32() else {
            continue;
        };
        let v = row_f32 / steps_f32;
        let mut vertices = Vec::with_capacity(steps.saturating_add(1));

        for column in 0..=steps {
            let Some(column_f32) = column.to_f32() else {
                continue;
            };
            let u = column_f32 / steps_f32;
            let vertex = patch.evaluate(u, v);
            vertices.push(transform_mesh_vertex(vertex, transform));
        }

        rows.push(vertices);
    }

    rows
}

fn transform_mesh_vertex(vertex: MeshVertex, transform: &Transform) -> MeshVertex {
    MeshVertex {
        point: transformed_point(vertex.point, transform),
        color: vertex.color,
    }
}

fn bounded_raster_dimension(value: f32, max_dimension: usize) -> usize {
    let ceil_value = value.ceil().max(1.0);
    match ceil_value.to_usize() {
        Some(dimension) => dimension.min(max_dimension),
        None => max_dimension,
    }
}

fn interpolate_triangle_color(triangle: [MeshVertex; 3], w0: f32, w1: f32, w2: f32) -> Color {
    let [v0, v1, v2] = triangle;
    Color::from_rgba(
        v0.color.r * w0 + v1.color.r * w1 + v2.color.r * w2,
        v0.color.g * w0 + v1.color.g * w1 + v2.color.g * w2,
        v0.color.b * w0 + v1.color.b * w1 + v2.color.b * w2,
        v0.color.a * w0 + v1.color.a * w1 + v2.color.a * w2,
    )
}

fn write_rgba_pixel(pixels: &mut [u8], width: usize, x: f32, y: f32, color: Color) {
    let Some(x) = x.to_usize() else {
        return;
    };
    let Some(y) = y.to_usize() else {
        return;
    };

    let pixel_index = y.saturating_mul(width).saturating_add(x).saturating_mul(4);
    let end_index = pixel_index.saturating_add(4);
    let Some(pixel) = pixels.get_mut(pixel_index..end_index) else {
        return;
    };
    let [red, green, blue, alpha] = color_to_rgba8(color);

    if let [r, g, b, a] = pixel {
        *r = red;
        *g = green;
        *b = blue;
        *a = alpha;
    }
}

fn color_to_rgba8(color: Color) -> [u8; 4] {
    [
        float_channel_to_u8(color.r),
        float_channel_to_u8(color.g),
        float_channel_to_u8(color.b),
        float_channel_to_u8(color.a),
    ]
}

fn float_channel_to_u8(channel: f32) -> u8 {
    let scaled = (channel.clamp(0.0, 1.0) * 255.0).round();
    match scaled.to_u8() {
        Some(value) => value,
        None => {
            if scaled.is_sign_negative() {
                0
            } else {
                u8::MAX
            }
        }
    }
}

fn barycentric_weights(triangle: [MeshVertex; 3], x: f32, y: f32) -> Option<(f32, f32, f32)> {
    let [a, b, c] = triangle;
    let denominator = (b.point.y - c.point.y) * (a.point.x - c.point.x)
        + (c.point.x - b.point.x) * (a.point.y - c.point.y);

    if denominator.abs() <= f32::EPSILON {
        return None;
    }

    let w0 = ((b.point.y - c.point.y) * (x - c.point.x)
        + (c.point.x - b.point.x) * (y - c.point.y))
        / denominator;
    let w1 = ((c.point.y - a.point.y) * (x - c.point.x)
        + (a.point.x - c.point.x) * (y - c.point.y))
        / denominator;
    let w2 = 1.0 - w0 - w1;
    let epsilon = -0.001;

    if w0 >= epsilon && w1 >= epsilon && w2 >= epsilon {
        Some((w0, w1, w2))
    } else {
        None
    }
}

fn triangle_scan_bounds(
    triangle: [MeshVertex; 3],
    bounds: &Rect,
    width: usize,
    height: usize,
) -> Option<Rect> {
    let min_x = triangle
        .iter()
        .map(|vertex| vertex.point.x)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(bounds.left);
    let max_x = triangle
        .iter()
        .map(|vertex| vertex.point.x)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min(bounds.right);
    let min_y = triangle
        .iter()
        .map(|vertex| vertex.point.y)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(bounds.top);
    let max_y = triangle
        .iter()
        .map(|vertex| vertex.point.y)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min(bounds.bottom);

    if min_x >= max_x || min_y >= max_y {
        return None;
    }

    let max_width = width.to_f32().unwrap_or(f32::MAX);
    let max_height = height.to_f32().unwrap_or(f32::MAX);

    Some(Rect {
        left: (min_x - bounds.left).max(0.0).min(max_width),
        top: (min_y - bounds.top).max(0.0).min(max_height),
        right: (max_x - bounds.left).max(0.0).min(max_width),
        bottom: (max_y - bounds.top).max(0.0).min(max_height),
    })
}

#[cfg(test)]
mod tests;
