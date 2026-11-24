#![allow(clippy::expect_used)]

use pdf_color_space::color_space::ColorSpace;
use pdf_graphics::{color::Color, point::Point, transform::Transform};
use pdf_shading::{
    model::{MeshPatch, Shading, ShadingType},
    paint::{ShadingPaint, ShadingSource},
};

#[test]
fn patch_mesh_paints_its_transformed_bounds() {
    let shading = Shading::PatchMesh {
        shading_type: ShadingType::CoonsPatchMesh,
        color_space: ColorSpace::DeviceRGB,
        bbox: None,
        anti_alias: None,
        patches: vec![MeshPatch::Coons {
            control_points: [
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
            ],
            corner_colors: [
                Color::from_rgb(1.0, 0.0, 0.0),
                Color::from_rgb(0.0, 1.0, 0.0),
                Color::from_rgb(0.0, 0.0, 1.0),
                Color::from_rgb(1.0, 1.0, 0.0),
            ],
        }],
    };

    let paint = shading
        .paint(&Transform::from_translate(10.0, 20.0), false)
        .expect("patch mesh should build raster paint");
    let ShadingPaint {
        source: ShadingSource::RasterImage(image),
        transform,
    } = paint
    else {
        return;
    };

    assert_eq!(transform, Transform::from_translate(10.0, 20.0));
    assert_eq!((image.width, image.height), (100, 100));
    let (pixels, _) = image.data.as_chunks::<4>();
    assert_eq!(pixels.len(), 100 * 100);
    assert!(pixels.iter().all(|[_, _, _, alpha]| *alpha == u8::MAX));
}
