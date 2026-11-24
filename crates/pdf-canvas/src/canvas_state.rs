//! PDF graphics-state values retained across saved states.
use pdf_graphics::{CanvasPaint, color::Color, transform::Transform};
use pdf_resources::{color_transfer::ColorTransferTables, resources::Resources};
use std::sync::Arc;

use crate::{
    CanvasPath,
    current_color::{CurrentColor, PaintTarget},
    mask_layer::MaskLayer,
    text_state::TextState,
};

/// Represents the complete graphics state for a PDF canvas, including
/// transformation, color, stroke, text, and pattern information.
#[derive(Clone)]
pub(crate) struct CanvasState {
    /// The current transformation matrix, mapping user space to device space.
    pub transform: Transform,
    /// Default user-space mapping of the current content stream, before its `cm` operators.
    pub pattern_parent_transform: Transform,
    /// Selected soft mask and the device transform captured when it was selected.
    pub soft_mask: Option<MaskLayer>,
    /// Transfer tables applied to painted image colors; `None` leaves them unchanged.
    pub transfer: Option<Arc<ColorTransferTables>>,
    /// Paint properties used for paths and text glyphs.
    pub paint: CanvasPaint,
    /// The current text state, encapsulating font, size, and text matrix.
    pub text_state: TextState,
    /// The current clipping path, if any, restricting drawing to a region.
    pub clip_path: Option<CanvasPath<'static>>,
    /// The current resource dictionary, overriding the page's resources if set.
    pub resources: Option<Arc<Resources>>,
    /// The current color space and pattern for stroking operations.
    pub stroke: CurrentColor,
    /// The current color space and pattern for non-stroking operations.
    pub fill: CurrentColor,
}

impl CanvasState {
    /// Returns the color space and pattern selected for `target`.
    pub(crate) const fn current_color(&self, target: PaintTarget) -> &CurrentColor {
        match target {
            PaintTarget::Stroke => &self.stroke,
            PaintTarget::Fill => &self.fill,
        }
    }

    /// Returns the color space and pattern selected for `target`, to change them.
    pub(crate) const fn current_color_mut(&mut self, target: PaintTarget) -> &mut CurrentColor {
        match target {
            PaintTarget::Stroke => &mut self.stroke,
            PaintTarget::Fill => &mut self.fill,
        }
    }

    /// Returns the numeric color `target` paints with, including its alpha constant.
    pub(crate) const fn paint_color(&self, target: PaintTarget) -> Color {
        match target {
            PaintTarget::Stroke => self.paint.stroke_color,
            PaintTarget::Fill => self.paint.fill_color,
        }
    }

    /// Replaces the numeric color `target` paints with, keeping its alpha constant.
    pub(crate) fn set_paint_color(&mut self, target: PaintTarget, color: Color) {
        match target {
            PaintTarget::Stroke => self.paint.set_stroke_color(color),
            PaintTarget::Fill => self.paint.set_fill_color(color),
        }
    }
}

impl Default for CanvasState {
    /// Creates the initial state with the documented rendering defaults.
    fn default() -> Self {
        Self {
            transform: Transform::identity(),
            pattern_parent_transform: Transform::identity(),
            soft_mask: None,
            transfer: None,
            paint: CanvasPaint::default(),
            text_state: TextState::default(),
            clip_path: None,
            resources: None,
            stroke: CurrentColor::default(),
            fill: CurrentColor::default(),
        }
    }
}
