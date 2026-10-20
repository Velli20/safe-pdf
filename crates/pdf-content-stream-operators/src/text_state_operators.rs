use std::sync::Arc;

use crate::{
    OperandContext,
    error::PdfOperatorError,
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
};
use num_traits::FromPrimitive;
use pdf_graphics::TextRenderingMode;
use pdf_object_reader::{ReadResult, pdf_string::PdfString};

/// Sets the character spacing, `Tc`, which is a number expressed in unscaled text space units.
#[derive(Debug, Clone, PartialEq)]
pub struct SetCharacterSpacing {
    /// The character spacing. Added to the horizontal displacement otherwise produced by showing a glyph.
    spacing: f32,
}

impl SetCharacterSpacing {
    pub fn new(spacing: f32) -> Self {
        Self { spacing }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let spacing = operands.at(0)?;
        Ok(Self::new(spacing))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_character_spacing(self.spacing)
    }
}

/// Sets the word spacing, `Tw`, which is a number expressed in unscaled text space units.
/// Word spacing is used by the `Tj`, `'`, and `"` operators.
#[derive(Debug, Clone, PartialEq)]
pub struct SetWordSpacing {
    /// The word spacing. Added to the character spacing when the character is a space (char code 32).
    spacing: f32,
}

impl SetWordSpacing {
    pub fn new(spacing: f32) -> Self {
        Self { spacing }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let spacing = operands.at(0)?;
        Ok(Self::new(spacing))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_word_spacing(self.spacing)
    }
}

/// Sets the horizontal scaling, `Tz`, which adjusts the width of glyphs by
/// stretching or compressing them horizontally.
#[derive(Debug, Clone, PartialEq)]
pub struct SetHorizontalScaling {
    /// The horizontal scaling factor as a percentage (e.g., 100.0 for 100% - no scaling).
    scale: f32,
}

impl SetHorizontalScaling {
    pub fn new(scale: f32) -> Self {
        Self { scale }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let scale = operands.at(0)?;
        Ok(Self::new(scale))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_horizontal_text_scaling(self.scale)
    }
}

/// Sets the text leading, `TL`, which is the vertical distance between the baselines of
/// adjacent lines of text.
#[derive(Debug, Clone, PartialEq)]
pub struct SetLeading {
    /// The text leading, in unscaled text space units.
    leading: f32,
}

impl SetLeading {
    /// Creates a leading operator with the given baseline distance.
    pub fn new(leading: f32) -> Self {
        Self { leading }
    }

    /// Returns the baseline distance in unscaled text-space units.
    pub const fn leading(&self) -> f32 {
        self.leading
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let leading = operands.at(0)?;
        Ok(Self::new(leading))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_text_leading(self.leading)
    }
}

/// Sets the text font, `Tf`, to a font resource in the resource dictionary and the text
/// font size, `Tfs`, in unscaled text space units.
#[derive(Debug, Clone, PartialEq)]
pub struct SetFont {
    /// The name of the font resource.
    name: Arc<[u8]>,
    /// The font size.
    size: f32,
}

impl SetFont {
    pub fn new(name: Arc<[u8]>, size: f32) -> Self {
        Self { name, size }
    }

    /// Returns the font resource name.
    pub fn name(&self) -> &[u8] {
        &self.name
    }

    /// Returns the font size.
    pub const fn size(&self) -> f32 {
        self.size
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let name = operands.at::<PdfString>(0)?.bytes;
        let size = operands.at(1)?;
        Ok(Self::new(name, size))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_font_and_size(&self.name, self.size)
    }
}

/// Sets the text rendering mode, which determines whether text is filled, stroked,
/// used as a clipping path, or some combination.
#[derive(Debug, Clone, PartialEq)]
pub struct SetRenderingMode {
    /// The rendering mode.
    mode: TextRenderingMode,
}

impl SetRenderingMode {
    pub fn new(mode: u8) -> Result<Self, PdfOperatorError> {
        match TextRenderingMode::from_u8(mode) {
            Some(mode) => Ok(Self { mode }),
            None => Err(PdfOperatorError::InvalidOperandValue {
                expected: "One of the valid text rendering modes (0-7)",
                found: mode.to_string(),
            }),
        }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let mode = operands.at(0)?;
        Ok(Self::new(mode)?)
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_text_rendering_mode(self.mode)
    }
}

/// Sets the text rise, `Ts`, which specifies the vertical distance
/// to shift the baseline of text relative to the current baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct SetTextRise {
    /// The text rise, in unscaled text space units. A positive value moves the baseline up.
    rise: f32,
}

impl SetTextRise {
    pub fn new(rise: f32) -> Self {
        Self { rise }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let rise = operands.at(0)?;
        Ok(Self::new(rise))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_text_rise(self.rise)
    }
}
