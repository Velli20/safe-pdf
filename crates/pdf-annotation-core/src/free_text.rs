//! Plain free-text editing without PDF appearance-stream generation.
use std::sync::Arc;

use crate::{models::Point, pdf_data::BorderEffect, style::ResolvedStyle};
use pdf_graphics::{Translate, polyline::Polyline, rect::Rect};
use serde::{Deserialize, Serialize};

/// Styled page text, distinct from an anchored comment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct FreeText {
    /// Page-space placement.
    pub bounds: Rect,
    /// Current Unicode text.
    pub text: String,
    /// Current text and border style.
    pub style: ResolvedStyle,
    /// Rich text retained without claiming a rich-text editor or renderer.
    pub rich_contents: Option<Arc<[u8]>>,
    /// Optional open two- or three-point callout, translated with the annotation.
    pub callout: Option<Polyline>,
    /// Retained border effect, unsupported by the plain-text editor.
    pub border_effect: Option<BorderEffect>,
}

impl FreeText {
    /// Whether the legacy plain-text edit can preserve this annotation's semantics.
    /// This query does not mutate state or panic; annotation flags are checked separately.
    pub fn is_plain(&self) -> bool {
        self.rich_contents.is_none() && self.callout.is_none() && self.border_effect.is_none()
    }

    pub(crate) fn translate(&mut self, delta: Point) {
        self.bounds.translate(delta);
        self.callout.translate(delta);
    }
}
