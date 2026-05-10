use pdf_graphics::{color::Color, point::Point, rect::Rect, transform::Transform};

use super::{
    MeshPatchRef, patch_mesh_bounds, patch_subdivision, rasterize_patch_mesh, rasterize_triangle,
};

fn coons_patch() -> MeshPatchRef<'static> {
    static CONTROL_POINTS: [Point; 12] = [
        Point::new(0.0, 0.0),
        Point::new(25.0, 0.0),
        Point::new(75.0, 0.0),
        Point::new(100.0, 0.0),
        Point::new(100.0, 25.0),
        Point::new(100.0, 75.0),
        Point::new(100.0, 100.0),
        Point::new(75.0, 100.0),
        Point::new(25.0, 100.0),
        Point::new(0.0, 100.0),
        Point::new(0.0, 75.0),
        Point::new(0.0, 25.0),
    ];
    static CORNER_COLORS: [Color; 4] = [
        Color::from_rgb(1.0, 0.0, 0.0),
        Color::from_rgb(0.0, 1.0, 0.0),
        Color::from_rgb(0.0, 0.0, 1.0),
        Color::from_rgb(1.0, 1.0, 0.0),
    ];

    MeshPatchRef::Coons {
        control_points: &CONTROL_POINTS,
        corner_colors: &CORNER_COLORS,
    }
}

#[test]
fn patch_mesh_bounds_applies_transform() {
    let transform = Transform::from_translate(10.0, 20.0);
    let bounds = patch_mesh_bounds(std::iter::once(coons_patch()), &transform);

    assert_eq!(
        bounds,
        Some(Rect {
            left: 10.0,
            top: 20.0,
            right: 110.0,
            bottom: 120.0,
        })
    );
}

#[test]
fn patch_subdivision_scales_with_patch_size() {
    let transform = Transform::identity();
    let subdivision = patch_subdivision(coons_patch(), &transform);

    assert!(subdivision >= 8);
}

#[test]
fn rasterize_patch_mesh_returns_expected_image_metadata() {
    let bounds = Rect {
        left: 0.0,
        top: 0.0,
        right: 100.0,
        bottom: 100.0,
    };
    let patch = coons_patch();

    let raster = rasterize_patch_mesh(std::iter::once(patch), bounds, &Transform::identity(), 256);

    assert_eq!(raster.bounds, bounds);
    assert_eq!(raster.width, 100);
    assert_eq!(raster.height, 100);
    assert_eq!(raster.pixels.len(), 100 * 100 * 4);
}

#[test]
fn rasterize_triangle_writes_pixels() {
    let mut pixels = vec![255_u8; 4 * 4 * 4];
    let bounds = Rect {
        left: 0.0,
        top: 0.0,
        right: 4.0,
        bottom: 4.0,
    };
    let triangle = [
        super::MeshVertex {
            point: Point::new(0.0, 0.0),
            color: Color::from_rgb(1.0, 0.0, 0.0),
        },
        super::MeshVertex {
            point: Point::new(3.0, 0.0),
            color: Color::from_rgb(0.0, 1.0, 0.0),
        },
        super::MeshVertex {
            point: Point::new(0.0, 3.0),
            color: Color::from_rgb(0.0, 0.0, 1.0),
        },
    ];

    rasterize_triangle(&mut pixels, 4, 4, &bounds, triangle);

    assert!(pixels.iter().any(|component| *component != 255));
}
