#![allow(clippy::expect_used)]

use pdf_color_space::color_space::ColorSpace;
use pdf_graphics::{color::Color, point::Point, rect::Rect, transform::Transform};
use pdf_shading::{
    model::{MeshTriangle, MeshVertex, Shading},
    paint::{ShadingPaint, build_shading_paint},
};

#[test]
fn builds_transformed_free_form_triangle_raster_paint() {
    let shading = Shading::FreeFormTriangleMesh {
        color_space: ColorSpace::DeviceRGB,
        bbox: None,
        anti_alias: None,
        triangles: vec![MeshTriangle {
            vertices: [
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
            ],
        }],
    };

    let paint = build_shading_paint(&shading, Some(Transform::from_translate(2.0, 3.0)))
        .expect("free-form triangle mesh should build raster paint");
    assert!(matches!(paint, ShadingPaint::RasterImage { .. }));
    let ShadingPaint::RasterImage {
        pixels,
        width,
        height,
        dest_rect,
        transform,
    } = paint
    else {
        return;
    };

    assert_eq!(width, 4);
    assert_eq!(height, 4);
    assert_eq!(dest_rect.left, 2.0);
    assert_eq!(dest_rect.top, 3.0);
    assert_eq!(dest_rect.right, 6.0);
    assert_eq!(dest_rect.bottom, 7.0);
    assert_eq!(transform, None);
    assert!(
        pixels
            .chunks_exact(4)
            .any(|pixel| pixel.get(3) == Some(&u8::MAX))
    );
    assert!(pixels.chunks_exact(4).any(|pixel| pixel == [0, 0, 0, 0]));
}

#[test]
fn free_form_triangle_mesh_falls_back_from_empty_bbox_to_geometry() {
    let shading = Shading::FreeFormTriangleMesh {
        color_space: ColorSpace::DeviceRGB,
        bbox: Some(Rect {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        }),
        anti_alias: None,
        triangles: vec![MeshTriangle {
            vertices: [
                MeshVertex {
                    point: Point::new(10.0, 20.0),
                    color: Color::from_rgb(1.0, 0.0, 0.0),
                },
                MeshVertex {
                    point: Point::new(30.0, 20.0),
                    color: Color::from_rgb(0.0, 1.0, 0.0),
                },
                MeshVertex {
                    point: Point::new(10.0, 40.0),
                    color: Color::from_rgb(0.0, 0.0, 1.0),
                },
            ],
        }],
    };

    let paint =
        build_shading_paint(&shading, None).expect("triangle geometry should provide bounds");
    let ShadingPaint::RasterImage { dest_rect, .. } = paint else {
        return;
    };

    assert_eq!(dest_rect.left, 10.0);
    assert_eq!(dest_rect.top, 20.0);
    assert_eq!(dest_rect.right, 30.0);
    assert_eq!(dest_rect.bottom, 40.0);
}

#[test]
fn fully_degenerate_free_form_mesh_builds_transparent_paint() {
    let vertex = MeshVertex {
        point: Point::new(5.0, 5.0),
        color: Color::from_rgb(1.0, 0.0, 0.0),
    };
    let shading = Shading::FreeFormTriangleMesh {
        color_space: ColorSpace::DeviceRGB,
        bbox: None,
        anti_alias: None,
        triangles: vec![MeshTriangle {
            vertices: [vertex; 3],
        }],
    };

    let paint = build_shading_paint(&shading, None).expect("degenerate mesh should be a no-op");
    let ShadingPaint::RasterImage {
        pixels,
        width,
        height,
        ..
    } = paint
    else {
        return;
    };

    assert_eq!(width, 1);
    assert_eq!(height, 1);
    assert_eq!(pixels.as_ref(), [0, 0, 0, 0]);
}
