use num_traits::ToPrimitive;

use crate::{color::Color, rect::Rect, transform::Transform};

use super::{MeshPatchRef, MeshVertex, patch_subdivision, tessellate_patch};

/// A rasterized mesh image in device space.
#[derive(Debug, Clone, PartialEq)]
pub struct RasterizedPatchMesh {
    pub pixels: Vec<u8>,
    pub bounds: Rect,
    pub width: usize,
    pub height: usize,
}

/// Rasterizes a sequence of patches into an RGBA image.
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
    let mut pixels = vec![255u8; width.saturating_mul(height).saturating_mul(4)];

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

/// Rasterizes a single triangle into an RGBA image buffer.
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

    let start_x = scan_bounds.left.to_usize().unwrap_or(width).min(width);
    let end_x = scan_bounds.right.to_usize().unwrap_or(width).min(width);
    let start_y = scan_bounds.top.to_usize().unwrap_or(height).min(height);
    let end_y = scan_bounds.bottom.to_usize().unwrap_or(height).min(height);

    let Some(start_x_f32) = start_x.to_f32() else {
        return;
    };
    let Some(start_y_f32) = start_y.to_f32() else {
        return;
    };

    let mut sample_y = bounds.top + start_y_f32 + 0.5;
    for y in start_y..end_y {
        let mut sample_x = bounds.left + start_x_f32 + 0.5;
        for x in start_x..end_x {
            if let Some((w0, w1, w2)) = barycentric_weights(triangle, sample_x, sample_y) {
                let color = interpolate_triangle_color(triangle, w0, w1, w2);
                write_rgba_pixel(pixels, width, x, y, color);
            }

            sample_x += 1.0;
        }

        sample_y += 1.0;
    }
}

fn interpolate_triangle_color(triangle: [MeshVertex; 3], w0: f32, w1: f32, w2: f32) -> Color {
    Color::from_rgba(
        triangle[0].color.r * w0 + triangle[1].color.r * w1 + triangle[2].color.r * w2,
        triangle[0].color.g * w0 + triangle[1].color.g * w1 + triangle[2].color.g * w2,
        triangle[0].color.b * w0 + triangle[1].color.b * w1 + triangle[2].color.b * w2,
        triangle[0].color.a * w0 + triangle[1].color.a * w1 + triangle[2].color.a * w2,
    )
}

fn write_rgba_pixel(pixels: &mut [u8], width: usize, x: usize, y: usize, color: Color) {
    let pixel_index = y.saturating_mul(width).saturating_add(x).saturating_mul(4);
    let Some(pixel) = pixels.get_mut(pixel_index..pixel_index.saturating_add(4)) else {
        return;
    };

    let [red, green, blue, alpha] = color_to_rgba8(color);
    pixel[0] = red;
    pixel[1] = green;
    pixel[2] = blue;
    pixel[3] = alpha;
}

fn color_to_rgba8(color: Color) -> [u8; 4] {
    [
        float_channel_to_u8(color.r),
        float_channel_to_u8(color.g),
        float_channel_to_u8(color.b),
        float_channel_to_u8(color.a),
    ]
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

fn bounded_raster_dimension(value: f32, max_dimension: usize) -> usize {
    let ceil_value = value.ceil().max(1.0);
    match ceil_value.to_usize() {
        Some(dimension) => dimension.min(max_dimension),
        None => max_dimension,
    }
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
