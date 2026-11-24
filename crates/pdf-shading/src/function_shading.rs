//! Type 1 function-based shadings, sampled into a raster over their domain.

use pdf_color_space::color_space::ColorSpace;
use pdf_function::function::Function;
use pdf_graphics::{color::Color, point::Point, rect::Rect, transform::Transform};
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

use crate::{error::PdfShadingError, paint::ShadingPaint, pixel_grid::PixelGrid};

/// Largest raster edge sampled for a function-based shading, bounding per-pixel function calls.
const MAX_RASTER_DIMENSION: u16 = 1024;

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
    /// The function domain, whose top edge is its lowest y.
    pub domain: Rect,
    /// Maps the domain into shading space.
    pub matrix: Transform,
    /// One two-in, n-out function, or n two-in, one-out functions.
    pub functions: Vec<Function>,
}

/// Reads a Type 1 function-based shading dictionary.
///
/// A background whose components the color space rejects is ignored rather than
/// failing the shading.
impl FromPdfObject for FunctionShading {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let color_space = context.required::<ColorSpace>(b"ColorSpace")?;
        let functions = context.optional_list::<Function>(b"Function")?;
        if functions.is_empty() {
            return Err(PdfShadingError::MissingRequiredEntry { entry: "Function" }.into());
        }
        Ok(Self {
            background: context
                .optional::<Vec<f32>>(b"Background")?
                .and_then(|components| color_space.apply(&components).ok()),
            color_space,
            bbox: context
                .lenient::<Rect>(b"BBox")
                .map(|bbox| bbox.normalized()),
            anti_alias: context.optional(b"AntiAlias")?,
            domain: context.optional(b"Domain")?.map_or(
                Rect::UNIT_RECT,
                |[left, right, top, bottom]: [f32; 4]| Rect {
                    left,
                    top,
                    right,
                    bottom,
                },
            ),
            matrix: context.optional(b"Matrix")?.unwrap_or_default(),
            functions,
        })
    }
}

impl FunctionShading {
    /// Builds a raster paint covering the domain, mapped by `/Matrix` and then `transform`.
    ///
    /// With `use_background`, a `/Background` also fills the part of `/BBox` outside the
    /// domain; otherwise nothing is painted outside the domain. An empty or nonfinite domain
    /// paints nothing.
    pub fn paint(
        &self,
        transform: &Transform,
        use_background: bool,
    ) -> Result<ShadingPaint, PdfShadingError> {
        if !self.domain.is_valid() {
            return Ok(ShadingPaint::transparent());
        }
        let background = self.background.filter(|_| use_background);
        let area = background
            .and(self.bbox)
            .map_or(self.domain, |bbox| self.domain_with_bbox(&bbox));
        let outside = background.unwrap_or(Color::TRANSPARENT);

        let to_device = transform.post_concatenated(&self.matrix);
        let grid = PixelGrid::new(area, to_device.axis_scales(), MAX_RASTER_DIMENSION);
        let pixels = grid
            .centers()
            .map(|center| {
                let color = if self.domain.contains(center) {
                    self.color_at(center)?
                } else {
                    outside
                };
                Ok(color.to_rgba8())
            })
            .collect::<Result<_, PdfShadingError>>()?;
        Ok(ShadingPaint::raster_image(
            grid.image(pixels, true),
            to_device.post_concatenated(&grid.placement()),
        )?)
    }

    /// Returns the smallest rectangle in domain space holding the domain and `bbox`.
    ///
    /// `bbox` is in shading space, so it is mapped back through `/Matrix`. A singular
    /// matrix or a nonfinite result leaves the domain alone.
    fn domain_with_bbox(&self, bbox: &Rect) -> Rect {
        self.matrix
            .try_inverse()
            .and_then(|inverse| inverse.try_map_rect(bbox))
            .map(|bbox| self.domain.union(&bbox))
            .ok()
            .filter(Rect::<f32>::is_valid)
            .unwrap_or(self.domain)
    }

    /// Evaluates the shading functions at a domain point and converts the result to a color.
    fn color_at(&self, Point { x, y }: Point) -> Result<Color, PdfShadingError> {
        let components = match self.functions.as_slice() {
            [function] => function.apply(&[x, y])?,
            functions => functions
                .iter()
                .map(|function| function.apply(&[x, y]))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect(),
        };
        Ok(self.color_space.apply(&components)?)
    }
}
