//! Entries shared by Type 2 axial and Type 3 radial shadings.

use pdf_color_space::color_space::ColorSpace;
use pdf_function::function::Function;
use pdf_graphics::{color::Color, interval::Interval, rect::Rect};
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

use crate::color_stops::ColorStops;

/// The color ramp of an axial or radial shading, apart from its `/Coords` geometry.
#[derive(Debug, Clone)]
pub struct GradientShading {
    /// The shading color space.
    pub color_space: ColorSpace,
    /// Sampled color stops used by render backends.
    pub color_stops: ColorStops,
    /// Whether the shading extends beyond its start and end.
    pub extend: [bool; 2],
    /// Background color painted where a shading pattern does not cover.
    pub background: Option<Color>,
    /// Optional bounding box in shading space.
    pub bbox: Option<Rect>,
}

impl GradientShading {
    /// Returns the color stops with each non-extended end bounded by the background when
    /// `use_background` is set, or by transparency otherwise.
    pub fn bounded_stops(&self, use_background: bool) -> ColorStops {
        let outside = self
            .background
            .filter(|_| use_background)
            .unwrap_or(Color::TRANSPARENT);
        self.color_stops.bounded(self.extend, outside)
    }
}

/// Reads the `/ColorSpace`, `/Domain`, `/Function`, `/Extend`, `/Background` and `/BBox`
/// entries of an axial or radial shading dictionary.
///
/// Both ends default to not extending. A background whose components the color space
/// rejects is ignored rather than failing the shading.
impl FromPdfObject for GradientShading {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let color_space = context.required::<ColorSpace>(b"ColorSpace")?;
        let domain = context
            .optional::<Interval>(b"Domain")?
            .unwrap_or(Interval::UNIT);
        let function = context.required::<Function>(b"Function")?;
        Ok(Self {
            color_stops: ColorStops::from_function_domain(&function, &color_space, domain)?,
            extend: context.optional(b"Extend")?.unwrap_or_default(),
            background: context
                .optional::<Vec<f32>>(b"Background")?
                .and_then(|components| color_space.apply(&components).ok()),
            bbox: context.dictionary().optional_bbox(context.source())?,
            color_space,
        })
    }
}
