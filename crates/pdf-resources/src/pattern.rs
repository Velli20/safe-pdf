use pdf_content_stream::{ContentStream, ContentStreamIdAllocator};
use pdf_graphics::{rect::Rect, transform::Transform};
use pdf_object::{
    object_lookup::ObjectLookupExt, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};
use pdf_shading::model::Shading;

use crate::{
    error::PdfPagesError,
    external_graphics_state::ExternalGraphicsState,
    matrix::Matrix,
    object_reader::{ReadCycleTracker, ReadFromDictionary},
    resource_cache::ResourceCache,
    resources::Resources,
};

/// PaintType for tiling patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintType {
    /// Colored tiling pattern.
    Colored = 1,
    /// Uncolored tiling pattern.
    Uncolored = 2,
}

impl TryFrom<i32> for PaintType {
    type Error = PdfPagesError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(PaintType::Colored),
            2 => Ok(PaintType::Uncolored),
            _ => Err(PdfPagesError::InvalidPaintType { value }),
        }
    }
}

/// Represents the type of a PDF Pattern object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternType {
    /// Tiling pattern.
    Tiling = 1,
    /// Shading pattern.
    Shading = 2,
}

impl TryFrom<i32> for PatternType {
    type Error = PdfPagesError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(PatternType::Tiling),
            2 => Ok(PatternType::Shading),
            _ => Err(PdfPagesError::InvalidPatternType { value }),
        }
    }
}

/// Represents the `/TilingType` entry, which controls the spacing of tiles
/// in a tiling pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TilingType {
    /// Constant spacing.
    ConstantSpacing = 1,
    /// No distortion.
    NoDistortion = 2,
    /// Constant spacing and faster tiling.
    ConstantSpacingFast = 3,
}

impl TryFrom<i32> for TilingType {
    type Error = PdfPagesError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(TilingType::ConstantSpacing),
            2 => Ok(TilingType::NoDistortion),
            3 => Ok(TilingType::ConstantSpacingFast),
            _ => Err(PdfPagesError::InvalidTilingType { value }),
        }
    }
}

/// Represents a PDF Pattern object, which can be either a tiling pattern or a shading pattern.
///
/// Patterns are used as "colors" for filling or stroking paths, allowing for repeating
/// graphical figures or smooth color transitions (gradients) to be used.
#[allow(clippy::large_enum_variant)]
pub enum Pattern {
    /// A tiling pattern, which consists of a small graphical figure (a "pattern cell")
    /// that is replicated at fixed intervals to fill an area.
    Tiling {
        /// Specifies how the pattern's color is determined.
        paint_type: PaintType,
        /// Controls how the spacing of tiles is adjusted.
        tiling_type: TilingType,
        /// The bounding box of the pattern cell, defining its size.
        bbox: Rect,
        /// The horizontal spacing between adjacent tiles.
        x_step: f32,
        /// The vertical spacing between adjacent tiles.
        y_step: f32,
        /// An optional transformation matrix to be applied to the pattern.
        matrix: Option<Transform>,
        /// A dictionary of resources required by the pattern's content stream.
        resources: Resources,
        /// The content stream that defines the graphics of the pattern cell.
        content_stream: ContentStream,
    },
    /// A shading pattern, which defines a smooth transition between colors across an area.
    Shading {
        /// The shading object that defines the gradient fill.
        shading: Shading,
        /// An optional transformation matrix to be applied to the pattern.
        matrix: Option<Transform>,
        /// An optional external graphics state to apply when painting the pattern.
        ext_g_state: Option<ExternalGraphicsState>,
    },
}

impl Pattern {
    /// Reads and constructs a `Pattern` from a PDF object.
    ///
    /// This function parses a PDF pattern object, which can be either a tiling pattern or a shading pattern,
    /// from the provided `object` using the given `objects` resolver and `cache` for resource management.
    /// It extracts all required fields and sub-objects, handling both pattern types as defined by the PDF specification.
    ///
    /// # Parameters
    ///
    /// - `object`: The PDF object variant representing the pattern to parse.
    /// - `objects`: The object resolver used to resolve indirect references within the PDF.
    /// - `cache`: A mutable reference to the resource cache for resolving and storing resources.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the constructed `Pattern` on success, or a `PdfPagesError` if parsing fails.
    pub(crate) fn read(
        object: &ObjectVariant,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        cycle_tracker: &mut ReadCycleTracker,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Pattern, PdfPagesError> {
        let dictionary = object.try_dictionary(objects)?;

        let pattern_type = dictionary.required_number::<i32>("PatternType", objects)?;

        // Read the transformation matrix for the pattern. Defaults to identity.
        let matrix = Matrix::from_dictionary(dictionary, objects)?;

        match PatternType::try_from(pattern_type)? {
            PatternType::Tiling => {
                // Read the `/PaintType` entry.
                let paint_type_int = dictionary.required_number::<i32>("PaintType", objects)?;

                let paint_type = PaintType::try_from(paint_type_int)?;

                // Read the `/TilingType` entry.
                let tiling_type_int = dictionary.required_number::<i32>("TilingType", objects)?;
                let tiling_type = TilingType::try_from(tiling_type_int)?;

                // Read the `/BBox` entry.
                let bbox = dictionary
                    .required_array_of::<f32, 4>("BBox", objects)?
                    .into();

                // Read the `/XStep` entry.
                let x_step = dictionary.required_number::<f32>("XStep", objects)?;

                // Read the `/YStep` entry.
                let y_step = dictionary.required_number::<f32>("YStep", objects)?;

                // Read the `/Resources` entry. Needed by the pattern's content stream.
                let parsed_resources =
                    Resources::read(dictionary, objects, cache, cycle_tracker, id_allocator)?;
                let mut resources = Resources::default();
                if let Some(parsed) = parsed_resources {
                    resources = parsed;
                }

                let content_stream = ContentStream::new(object, objects, id_allocator)?;

                Ok(Pattern::Tiling {
                    paint_type,
                    tiling_type,
                    bbox,
                    x_step,
                    y_step,
                    matrix,
                    resources,
                    content_stream,
                })
            }
            PatternType::Shading => {
                let shading_object = dictionary.get_or_err("Shading")?;
                // Read the shading object that defines the gradient fill.
                let shading = Shading::from_dictionary(shading_object, objects)?;

                // Read an external graphics state dictionary to apply when painting the pattern.
                let ext_g_state = match dictionary
                    .get("ExtGState")
                    .map(|obj| obj.try_dictionary(objects))
                    .transpose()?
                {
                    Some(ext) => ExternalGraphicsState::from_dictionary(
                        ext,
                        objects,
                        cache,
                        cycle_tracker,
                        id_allocator,
                    )?,
                    None => None,
                };

                Ok(Pattern::Shading {
                    shading,
                    matrix,
                    ext_g_state,
                })
            }
        }
    }
}
