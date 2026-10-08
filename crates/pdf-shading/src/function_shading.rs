//! Type 1 function-based shadings, sampled into a raster over their domain.

use num_traits::ToPrimitive;
use pdf_color_space::color_space::ColorSpace;
use pdf_function::function::{Function, FunctionImpl};
use pdf_graphics::{
    Image, PixelFormat, color::Color, rect::Rect, size::Size, transform::Transform,
};
use pdf_object_reader::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::{
    error::PdfShadingError,
    paint::{ShadingPaint, transparent_raster_paint},
    parse::{parse_functions, read_background, required_color_space},
};

/// Largest raster edge sampled for a function-based shading, bounding per-pixel function calls.
const MAX_RASTER_DIMENSION: f32 = 1024.0;

/// Bytes per pixel in the RGBA8 raster.
const RGBA_BYTES_PER_PIXEL: usize = 4;

/// Offset from a pixel's corner to its center, where each sample is taken.
const PIXEL_CENTER: f32 = 0.5;

/// Default `/Domain` of a function-based shading: the unit square.
const DEFAULT_DOMAIN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];

/// A Type 1 function-based shading: color is a function of the point `(x, y)` in its domain.
#[derive(Debug, Clone)]
pub struct FunctionShading {
    /// The shading color space.
    pub color_space: ColorSpace,
    /// Background color painted where a shading pattern's `/BBox` extends past the domain.
    pub background: Option<Color>,
    /// Optional bounding box in shading space.
    pub bbox: Option<Rect>,
    /// Optional anti-aliasing preference.
    pub anti_alias: Option<bool>,
    /// The function domain `[x0 x1 y0 y1]`.
    pub domain: [f32; 4],
    /// Maps the domain into shading space.
    pub matrix: Transform,
    /// One two-in, n-out function, or n two-in, one-out functions.
    pub functions: Vec<Function>,
}

impl FunctionShading {
    /// Parses a Type 1 function-based shading dictionary.
    pub(crate) fn parse(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, PdfShadingError> {
        let color_space = required_color_space(dictionary, objects)?;
        Ok(Self {
            background: read_background(dictionary, objects, &color_space)?,
            color_space,
            bbox: dictionary.optional_bbox(objects)?,
            anti_alias: dictionary.optional_boolean(b"AntiAlias", objects)?,
            domain: dictionary
                .optional_array_of::<f32, 4>(b"Domain", objects)?
                .unwrap_or(DEFAULT_DOMAIN),
            matrix: dictionary
                .optional_matrix(objects)?
                .unwrap_or_else(Transform::identity),
            functions: parse_functions(dictionary, objects)?,
        })
    }

    /// Builds a raster paint covering the domain, mapped by `/Matrix` and then `transform`.
    ///
    /// With `use_background`, a `/Background` also fills the part of `/BBox` outside the
    /// domain; otherwise nothing is painted outside the domain. An empty or nonfinite domain
    /// paints nothing.
    pub fn paint(
        &self,
        transform: Option<Transform>,
        use_background: bool,
    ) -> Result<ShadingPaint, PdfShadingError> {
        let domain = self.domain_rect();
        if !domain.is_valid() {
            return Ok(transparent_raster_paint());
        }
        let background = self.background.filter(|_| use_background);
        let area = match (background, &self.bbox) {
            (Some(_), Some(bbox)) => self.domain_with_bbox(&domain, bbox),
            _ => domain,
        };
        let to_device = transform
            .unwrap_or_else(Transform::identity)
            .post_concatenated(&self.matrix);
        let outside = background.unwrap_or(Color::TRANSPARENT);
        let image = self.rasterize(
            &domain,
            &area,
            Self::raster_size(&area, &to_device),
            outside,
        )?;
        ShadingPaint::raster_image(image, area, Some(to_device))
            .map_err(|error| PdfShadingError::UnsupportedFeature(error.to_string()))
    }

    /// Returns the smallest rectangle in domain space holding the domain and `bbox`.
    ///
    /// `bbox` is in shading space, so it is mapped back through `/Matrix`. A singular
    /// matrix or a nonfinite result leaves the domain alone.
    fn domain_with_bbox(&self, domain: &Rect, bbox: &Rect) -> Rect {
        let Ok(bbox) = self
            .matrix
            .try_inverse()
            .and_then(|inverse| inverse.try_map_rect(bbox))
        else {
            return *domain;
        };
        let area = Rect {
            left: domain.left.min(bbox.left),
            top: domain.top.min(bbox.top),
            right: domain.right.max(bbox.right),
            bottom: domain.bottom.max(bbox.bottom),
        };
        if area.is_valid() { area } else { *domain }
    }

    /// Returns the domain as a rectangle whose top edge is `y0`.
    fn domain_rect(&self) -> Rect {
        let [left, right, top, bottom] = self.domain;
        Rect {
            left,
            top,
            right,
            bottom,
        }
    }

    /// Returns the whole-pixel size the domain covers once mapped by `to_device`.
    ///
    /// Each edge is clamped to `1..=MAX_RASTER_DIMENSION`. A NaN extent stays NaN, which
    /// [`Self::rasterize`] rejects.
    fn raster_size(domain: &Rect, to_device: &Transform) -> Size {
        let Size {
            width: scale_x,
            height: scale_y,
        } = to_device.axis_scales();
        let pixels = |extent: f32| extent.ceil().clamp(1.0, MAX_RASTER_DIMENSION);
        Size::new(
            pixels(domain.width() * scale_x),
            pixels(domain.height() * scale_y),
        )
    }

    /// Samples the shading at pixel centers over `area`, row 0 nearest `area.top`.
    ///
    /// `size` holds whole pixel counts from [`Self::raster_size`]. Samples outside
    /// `domain` take `outside`.
    fn rasterize(
        &self,
        domain: &Rect,
        area: &Rect,
        size: Size,
        outside: Color,
    ) -> Result<Image, PdfShadingError> {
        let (Some(width), Some(height)) = (size.width.to_usize(), size.height.to_usize()) else {
            return Err(PdfShadingError::UnsupportedFeature(
                "invalid raster size".to_string(),
            ));
        };
        let mut data = Vec::with_capacity(
            width
                .saturating_mul(height)
                .saturating_mul(RGBA_BYTES_PER_PIXEL),
        );

        // Area units per pixel. `row` and `column` count pixels, advancing one at a time.
        let step_x = area.width() / size.width;
        let step_y = area.height() / size.height;
        let mut row = PIXEL_CENTER;
        while row < size.height {
            let y = area.top + row * step_y;
            let mut column = PIXEL_CENTER;
            while column < size.width {
                let x = area.left + column * step_x;
                let color = if domain.contains_point(x, y) {
                    self.color_at(x, y)?
                } else {
                    outside
                };
                data.extend_from_slice(&color.to_rgba8());
                column += 1.0;
            }
            row += 1.0;
        }

        Ok(Image {
            data: data.into(),
            width,
            height,
            pixel_format: PixelFormat::RGBA8888,
            interpolate: true,
        })
    }

    /// Evaluates the shading functions at `(x, y)` and converts the result to a color.
    fn color_at(&self, x: f32, y: f32) -> Result<Color, PdfShadingError> {
        let components = match self.functions.as_slice() {
            [function] => function.interpolate(&[x, y])?,
            functions => functions
                .iter()
                .map(|function| function.interpolate(&[x, y]))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect(),
        };
        Ok(self.color_space.apply(&components)?)
    }
}
