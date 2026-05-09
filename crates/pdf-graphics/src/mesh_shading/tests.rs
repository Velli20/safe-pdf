use super::{
    MeshPatchRef, MeshVertex, evaluate_coons_patch_vertex, evaluate_tensor_patch_vertex,
    patch_mesh_bounds, patch_subdivision, rasterize_patch_mesh, rasterize_triangle,
    tessellate_patch,
};
use crate::{color::Color, point::Point, rect::Rect, transform::Transform};

fn approx_eq(left: f32, right: f32) {
    assert!((left - right).abs() < 1e-5, "left={left}, right={right}");
}

fn assert_point(point: Point, x: f32, y: f32) {
    approx_eq(point.x, x);
    approx_eq(point.y, y);
}

fn assert_color(color: Color, r: f32, g: f32, b: f32, a: f32) {
    approx_eq(color.r, r);
    approx_eq(color.g, g);
    approx_eq(color.b, b);
    approx_eq(color.a, a);
}

fn corner_colors() -> [Color; 4] {
    [
        Color::from_rgb(1.0, 0.0, 0.0),
        Color::from_rgb(0.0, 1.0, 0.0),
        Color::from_rgb(0.0, 0.0, 1.0),
        Color::from_rgb(1.0, 1.0, 1.0),
    ]
}

fn coons_patch() -> [Point; 12] {
    [
        Point::new(0.0, 0.0),
        Point::new(1.0 / 3.0, 0.0),
        Point::new(2.0 / 3.0, 0.0),
        Point::new(1.0, 0.0),
        Point::new(1.0, 1.0 / 3.0),
        Point::new(1.0, 2.0 / 3.0),
        Point::new(1.0, 1.0),
        Point::new(2.0 / 3.0, 1.0),
        Point::new(1.0 / 3.0, 1.0),
        Point::new(0.0, 1.0),
        Point::new(0.0, 2.0 / 3.0),
        Point::new(0.0, 1.0 / 3.0),
    ]
}

fn tensor_patch() -> [Point; 16] {
    [
        Point::new(0.0, 0.0),
        Point::new(1.0, 0.0),
        Point::new(2.0, 0.0),
        Point::new(3.0, 0.0),
        Point::new(0.0, 1.0),
        Point::new(1.0, 1.0),
        Point::new(2.0, 1.0),
        Point::new(3.0, 1.0),
        Point::new(0.0, 2.0),
        Point::new(1.0, 2.0),
        Point::new(2.0, 2.0),
        Point::new(3.0, 2.0),
        Point::new(0.0, 3.0),
        Point::new(1.0, 3.0),
        Point::new(2.0, 3.0),
        Point::new(3.0, 3.0),
    ]
}

#[test]
fn coons_patch_returns_expected_corners() {
    let control_points = coons_patch();
    let colors = corner_colors();

    let top_left = evaluate_coons_patch_vertex(&control_points, &colors, 0.0, 0.0);
    assert_point(top_left.point, 0.0, 0.0);
    assert_color(top_left.color, 1.0, 0.0, 0.0, 1.0);

    let top_right = evaluate_coons_patch_vertex(&control_points, &colors, 1.0, 0.0);
    assert_point(top_right.point, 1.0, 0.0);
    assert_color(top_right.color, 0.0, 1.0, 0.0, 1.0);

    let bottom_right = evaluate_coons_patch_vertex(&control_points, &colors, 1.0, 1.0);
    assert_point(bottom_right.point, 1.0, 1.0);
    assert_color(bottom_right.color, 0.0, 0.0, 1.0, 1.0);

    let bottom_left = evaluate_coons_patch_vertex(&control_points, &colors, 0.0, 1.0);
    assert_point(bottom_left.point, 0.0, 1.0);
    assert_color(bottom_left.color, 1.0, 1.0, 1.0, 1.0);
}

#[test]
fn coons_patch_midpoint_matches_bilinear_rectangle() {
    let control_points = [
        Point::new(0.0, 0.0),
        Point::new(1.0, 0.0),
        Point::new(2.0, 0.0),
        Point::new(3.0, 0.0),
        Point::new(3.0, 1.0),
        Point::new(3.0, 2.0),
        Point::new(3.0, 3.0),
        Point::new(2.0, 3.0),
        Point::new(1.0, 3.0),
        Point::new(0.0, 3.0),
        Point::new(0.0, 2.0),
        Point::new(0.0, 1.0),
    ];
    let colors = corner_colors();

    let midpoint = evaluate_coons_patch_vertex(&control_points, &colors, 0.5, 0.5);

    assert_point(midpoint.point, 1.5, 1.5);
    assert_color(midpoint.color, 0.5, 0.5, 0.5, 1.0);
}

#[test]
fn tensor_patch_returns_expected_corners() {
    let control_points = tensor_patch();
    let colors = corner_colors();

    let top_left = evaluate_tensor_patch_vertex(&control_points, &colors, 0.0, 0.0);
    assert_point(top_left.point, 0.0, 0.0);
    assert_color(top_left.color, 1.0, 0.0, 0.0, 1.0);

    let top_right = evaluate_tensor_patch_vertex(&control_points, &colors, 1.0, 0.0);
    assert_point(top_right.point, 3.0, 0.0);
    assert_color(top_right.color, 0.0, 1.0, 0.0, 1.0);

    let bottom_right = evaluate_tensor_patch_vertex(&control_points, &colors, 1.0, 1.0);
    assert_point(bottom_right.point, 3.0, 3.0);
    assert_color(bottom_right.color, 0.0, 0.0, 1.0, 1.0);

    let bottom_left = evaluate_tensor_patch_vertex(&control_points, &colors, 0.0, 1.0);
    assert_point(bottom_left.point, 0.0, 3.0);
    assert_color(bottom_left.color, 1.0, 1.0, 1.0, 1.0);
}

#[test]
fn tensor_patch_midpoint_matches_planar_grid() {
    let control_points = tensor_patch();
    let colors = corner_colors();

    let midpoint = evaluate_tensor_patch_vertex(&control_points, &colors, 0.5, 0.5);

    assert_point(midpoint.point, 1.5, 1.5);
    assert_color(midpoint.color, 0.5, 0.5, 0.5, 1.0);
}

#[test]
fn patch_mesh_bounds_applies_transform() {
    let control_points = tensor_patch();
    let colors = corner_colors();
    let transform = Transform::from_row(2.0, 0.0, 0.0, 3.0, 10.0, 20.0);

    let bounds = patch_mesh_bounds(
        std::iter::once(MeshPatchRef::Tensor {
            control_points: &control_points,
            corner_colors: &colors,
        }),
        &transform,
    )
    .expect("bounds should exist");

    assert_eq!(
        bounds,
        Rect {
            left: 10.0,
            top: 20.0,
            right: 16.0,
            bottom: 29.0,
        }
    );
}

#[test]
fn patch_subdivision_tracks_extent() {
    let control_points = tensor_patch();
    let colors = corner_colors();
    let patch = MeshPatchRef::Tensor {
        control_points: &control_points,
        corner_colors: &colors,
    };
    let transform = Transform::from_scale(4.0, 4.0);

    assert_eq!(patch_subdivision(patch, &transform), 12);
}

#[test]
fn tessellate_patch_returns_transformed_triangles() {
    let control_points = tensor_patch();
    let colors = corner_colors();
    let patch = MeshPatchRef::Tensor {
        control_points: &control_points,
        corner_colors: &colors,
    };
    let transform = Transform::from_row(2.0, 0.0, 0.0, 3.0, 10.0, 20.0);

    let triangles = tessellate_patch(patch, &transform, 1);

    assert_eq!(triangles.len(), 2);
    assert_point(triangles[0][0].point, 10.0, 20.0);
    assert_point(triangles[0][1].point, 16.0, 20.0);
    assert_point(triangles[1][1].point, 16.0, 29.0);
}

#[test]
fn rasterize_triangle_colors_covered_pixel() {
    let mut pixels = vec![255u8; 4 * 4 * 4];
    let bounds = Rect {
        left: 0.0,
        top: 0.0,
        right: 4.0,
        bottom: 4.0,
    };
    let triangle = [
        MeshVertex {
            point: Point::new(0.0, 0.0),
            color: Color::from_rgb(1.0, 0.0, 0.0),
        },
        MeshVertex {
            point: Point::new(4.0, 0.0),
            color: Color::from_rgb(0.0, 1.0, 0.0),
        },
        MeshVertex {
            point: Point::new(0.0, 4.0),
            color: Color::from_rgb(0.0, 0.0, 1.0),
        },
    ];

    rasterize_triangle(&mut pixels, 4, 4, &bounds, triangle);

    let pixel = &pixels[0..4];
    assert_eq!(pixel, &[191, 32, 32, 255]);
}

#[test]
fn rasterize_patch_mesh_returns_expected_image_metadata() {
    let control_points = tensor_patch();
    let colors = corner_colors();
    let patch = MeshPatchRef::Tensor {
        control_points: &control_points,
        corner_colors: &colors,
    };
    let bounds = Rect {
        left: 0.0,
        top: 0.0,
        right: 3.0,
        bottom: 3.0,
    };

    let raster = rasterize_patch_mesh(std::iter::once(patch), bounds, &Transform::identity(), 2048);

    assert_eq!(raster.width, 3);
    assert_eq!(raster.height, 3);
    assert_eq!(raster.bounds, bounds);
    assert_eq!(raster.pixels.len(), 3 * 3 * 4);
}
