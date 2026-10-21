use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ObjectHandle, ReadResult};

use pdf_content_stream::ContentStream;
use pdf_graphics::{rect::Rect, transform::Transform};
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

impl FromPdfObject for PaintType {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match i32::from_pdf_object(context)? {
            1 => Ok(PaintType::Colored),
            2 => Ok(PaintType::Uncolored),
            value => Err(PdfPagesError::InvalidPaintType { value }.into()),
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

impl FromPdfObject for PatternType {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match i32::from_pdf_object(context)? {
            1 => Ok(PatternType::Tiling),
            2 => Ok(PatternType::Shading),
            value => Err(PdfPagesError::InvalidPatternType { value }.into()),
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

impl FromPdfObject for TilingType {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match i32::from_pdf_object(context)? {
            1 => Ok(TilingType::ConstantSpacing),
            2 => Ok(TilingType::NoDistortion),
            3 => Ok(TilingType::ConstantSpacingFast),
            value => Err(PdfPagesError::InvalidTilingType { value }.into()),
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

impl Pattern {
    /// Returns the matrix mapping pattern space into the parent stream's default user space.
    pub fn matrix(&self) -> Option<&Transform> {
        match self {
            Self::Tiling { matrix, .. } | Self::Shading { matrix, .. } => matrix.as_ref(),
        }
    }
}

impl FromPdfObject for Pattern {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let object = context.object();
        // Tiling patterns are streams and shading patterns are dictionaries; both expose the same entries.
        let mut context = context.dictionary()?;
        let matrix = context.optional::<Transform>(b"Matrix")?;
        match context.required::<PatternType>(b"PatternType")? {
            PatternType::Tiling => Ok(Pattern::Tiling {
                paint_type: context.required(b"PaintType")?,
                tiling_type: context.required(b"TilingType")?,
                bbox: context.required::<Rect>(b"BBox")?.normalized(),
                x_step: context.required(b"XStep")?,
                y_step: context.required(b"YStep")?,
                matrix,
                // Keep a deferred handle so resources that refer back to this pattern can finish decoding.
                resources: context
                    .optional_shared::<Resources>(b"Resources")?
                    .unwrap_or_else(|| ObjectHandle::from(Resources::default())),
                content_stream: context.read::<ContentStream>(object)?,
            }),
            PatternType::Shading => Ok(Pattern::Shading {
                shading: context.required::<Shading>(b"Shading")?,
                matrix,
                ext_g_state: context.optional::<ExternalGraphicsState>(b"ExtGState")?,
            }),
        }
    }
}
