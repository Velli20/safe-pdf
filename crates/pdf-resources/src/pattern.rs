use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ObjectHandle, ReadResult};

use pdf_content_stream::ContentStream;
use pdf_graphics::{rect::Rect, transform::Transform};
use pdf_object_reader::object_lookup::ObjectLookupExt;
use pdf_shading::model::Shading;

use crate::{
    error::PdfPagesError, external_graphics_state::ExternalGraphicsState, resources::Resources,
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
        resources: ObjectHandle<Resources>,
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

impl FromPdfObject for Pattern {
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let raw = context.object().object().clone();
        let object = raw.value();
        let objects = context.source();
        let dictionary = match object {
            pdf_object_reader::object_variant::ObjectVariant::Dictionary(dictionary) => dictionary,
            pdf_object_reader::object_variant::ObjectVariant::Stream(stream) => &stream.dictionary,
            other => {
                return Err(pdf_object_reader::object_error::ObjectError::TypeMismatch(
                    "Dictionary or Stream",
                    other.name(),
                )
                .into());
            }
        };

        let pattern_type = dictionary.required_number::<i32>(b"PatternType", objects)?;

        // Read the transformation matrix for the pattern. Defaults to identity.
        let matrix = dictionary.optional_matrix(objects)?;

        match PatternType::try_from(pattern_type)? {
            PatternType::Tiling => {
                // Read the `/PaintType` entry.
                let paint_type_int = dictionary.required_number::<i32>(b"PaintType", objects)?;

                let paint_type = PaintType::try_from(paint_type_int)?;

                // Read the `/TilingType` entry.
                let tiling_type_int = dictionary.required_number::<i32>(b"TilingType", objects)?;
                let tiling_type = TilingType::try_from(tiling_type_int)?;

                // Read the `/BBox` entry.
                let bbox = dictionary.required_bbox(objects)?;

                // Read the `/XStep` entry.
                let x_step = dictionary.required_number::<f32>(b"XStep", objects)?;

                // Read the `/YStep` entry.
                let y_step = dictionary.required_number::<f32>(b"YStep", objects)?;

                let content_stream = context.read::<ContentStream>(object)?;
                let resources = dictionary
                    .get(b"Resources")
                    .map(|value| context.read_shared(value))
                    .transpose()?
                    .unwrap_or_else(|| ObjectHandle::from(Resources::default()));

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
                let shading_object = dictionary.get_or_err(b"Shading")?;
                // Read the shading object that defines the gradient fill.
                let shading = context.read::<Shading>(shading_object)?;

                let ext_g_state = dictionary
                    .get(b"ExtGState")
                    .map(|value| context.read(value))
                    .transpose()?;

                Ok(Pattern::Shading {
                    shading,
                    matrix,
                    ext_g_state,
                })
            }
        }
    }
}
