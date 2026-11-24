#![allow(clippy::expect_used)]

use pdf_color_space::color_space::ColorSpace;
use pdf_graphics::{color::Color, point::Point, rect::Rect, transform::Transform};
use pdf_shading::{
    color_stops::ColorStops,
    model::{MeshTriangle, MeshVertex, Shading},
    paint::{ShadingPaint, ShadingSource},
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

    let paint = shading
        .paint(&Transform::from_translate(2.0, 3.0), false)
        .expect("free-form triangle mesh should build raster paint");
    let ShadingPaint {
        source: ShadingSource::RasterImage(image),
        transform,
    } = paint
    else {
        return;
    };

    assert_eq!(image.width, 4);
    assert_eq!(image.height, 4);
    assert_eq!(transform, Transform::from_translate(2.0, 3.0));
    assert!(
        image
            .data
            .as_chunks::<4>()
            .0
            .iter()
            .any(|[_, _, _, alpha]| *alpha == u8::MAX)
    );
    assert!(image.data.as_chunks::<4>().0.contains(&[0, 0, 0, 0]));
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

    let paint = shading
        .paint(&Transform::identity(), false)
        .expect("triangle geometry should provide bounds");
    let ShadingPaint {
        source: ShadingSource::RasterImage(image),
        transform,
    } = paint
    else {
        return;
    };

    assert_eq!(image.width, 20);
    assert_eq!(image.height, 20);
    assert_eq!(transform, Transform::from_translate(10.0, 20.0));
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

    let paint = shading
        .paint(&Transform::identity(), false)
        .expect("degenerate mesh should be a no-op");
    let ShadingSource::RasterImage(image) = paint.source else {
        return;
    };

    assert_eq!(image.width, 1);
    assert_eq!(image.height, 1);
    assert_eq!(image.data.as_ref(), [0, 0, 0, 0]);
}

#[test]
fn gradient_preparation_normalizes_colors() {
    let paint = ShadingPaint::linear_gradient(
        [0.0, 0.0, 10.0, 0.0],
        &ColorStops {
            positions: [0.0, 1.0].into(),
            colors: [
                Color::from_rgba(-1.0, 0.5, 2.0, 1.5),
                Color::from_rgb(0.0, 0.0, 0.0),
            ]
            .into(),
        },
        Transform::identity(),
    )
    .expect("gradient should be prepared");

    let ShadingSource::LinearGradient { stops, .. } = paint.source else {
        return;
    };
    assert_eq!(
        stops.colors.first(),
        Some(&Color::from_rgba(0.0, 0.5, 1.0, 1.0))
    );
}

#[test]
fn gradient_preparation_rejects_singular_transform() {
    let result = ShadingPaint::linear_gradient(
        [0.0, 0.0, 10.0, 0.0],
        &ColorStops {
            positions: [0.0, 1.0].into(),
            colors: [
                Color::from_rgb(0.0, 0.0, 0.0),
                Color::from_rgb(1.0, 1.0, 1.0),
            ]
            .into(),
        },
        Transform::from_scale(0.0, 1.0),
    );

    assert!(result.is_err());
}
