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
    let width = bounds.width().ceil().max(1.0) as usize;
    let height = bounds.height().ceil().max(1.0) as usize;
    let width = width.min(max_dimension);
    let height = height.min(max_dimension);
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

    for y in scan_bounds.start_y..scan_bounds.end_y {
        for x in scan_bounds.start_x..scan_bounds.end_x {
            let sample_x = bounds.left + x as f32 + 0.5;
            let sample_y = bounds.top + y as f32 + 0.5;

            if let Some((w0, w1, w2)) = barycentric_weights(triangle, sample_x, sample_y) {
                let color = interpolate_triangle_color(triangle, w0, w1, w2);
                write_rgba_pixel(pixels, width, x, y, color);
            }
        }
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
        (color.r.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.g.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.b.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.a.clamp(0.0, 1.0) * 255.0).round() as u8,
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
) -> Option<ScanBounds> {
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

    Some(ScanBounds {
        start_x: ((min_x - bounds.left).max(0.0) as usize).min(width),
        end_x: ((max_x - bounds.left).max(0.0) as usize).min(width),
        start_y: ((min_y - bounds.top).max(0.0) as usize).min(height),
        end_y: ((max_y - bounds.top).max(0.0) as usize).min(height),
    })
}

struct ScanBounds {
    start_x: usize,
    end_x: usize,
    start_y: usize,
    end_y: usize,
}
