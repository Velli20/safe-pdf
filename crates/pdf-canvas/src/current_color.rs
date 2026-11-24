//! The color space and pattern selected for stroking or non-stroking operations.
use std::sync::Arc;

use pdf_color_space::color_space::ColorSpace;
use pdf_resources::pattern::Pattern;

/// The painting operations a color applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaintTarget {
    /// Stroking operations.
    Stroke,
    /// Non-stroking operations.
    Fill,
}

/// The color space and pattern selected for one [`PaintTarget`].
///
/// The numeric color lives in [`pdf_graphics::CanvasPaint`], which backends read.
#[derive(Clone)]
pub(crate) struct CurrentColor {
    /// The space color operands are interpreted in.
    pub(crate) space: ColorSpace,
    /// The pattern painted with; `None` paints the numeric color.
    pub(crate) pattern: Option<Arc<Pattern>>,
}

impl Default for CurrentColor {
    fn default() -> Self {
        Self {
            space: ColorSpace::DeviceGray,
            pattern: None,
        }
    }
}
