//! PDF graphics-state values retained across saved states.
use pdf_color_space::color_space::ColorSpace;
use pdf_graphics::{CanvasPaint, transform::Transform};
use pdf_resources::{color_transfer::ColorTransferTables, pattern::Pattern, resources::Resources};
use std::sync::Arc;

use crate::{CanvasPath, mask_layer::MaskLayer, text_state::TextState};

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
    /// The current pattern used for filling.
    pub fill_pattern: Option<Arc<Pattern>>,
    /// The current pattern used for stroking.
    pub stroke_pattern: Option<Arc<Pattern>>,
    /// The current color space for stroking operations.
    pub stroke_color_space: Arc<ColorSpace>,
    /// The current color space for non-stroking operations.
    pub fill_color_space: Arc<ColorSpace>,
}

impl CanvasState {
    /// Selects a device color space for stroking or non-stroking operations.
    ///
    /// Color operators such as `rg` select their space on every use, so the current
    /// allocation is kept when it already holds `space`.
    pub(crate) fn select_device_color_space(&mut self, stroking: bool, space: ColorSpace) {
        let slot = if stroking {
            &mut self.stroke_color_space
        } else {
            &mut self.fill_color_space
        };
        let unchanged = matches!(
            (slot.as_ref(), &space),
            (ColorSpace::DeviceGray, ColorSpace::DeviceGray)
                | (ColorSpace::DeviceRGB, ColorSpace::DeviceRGB)
                | (ColorSpace::DeviceCMYK, ColorSpace::DeviceCMYK)
        );
        if !unchanged {
            *slot = Arc::new(space);
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
            fill_pattern: None,
            stroke_pattern: None,
            stroke_color_space: Arc::new(ColorSpace::DeviceGray),
            fill_color_space: Arc::new(ColorSpace::DeviceGray),
        }
    }
}
