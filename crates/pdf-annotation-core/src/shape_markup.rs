//! Local shapes for text markup drawn over selection quadrilaterals.
use crate::{
    models::Highlight,
    shapes::{Shape, ShapePaint},
    style::ResolvedStyle,
    subtypes::Markup,
};
use pdf_graphics::{polyline::Polyline, rect::Rect};

/// Fill alpha of highlight quadrilaterals.
const HIGHLIGHT_ALPHA: f32 = 0.35;

/// Line drawn across each quad of an underline, squiggly, or strikeout annotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MarkupLine {
    /// Straight line along the bottom edge.
    Underline,
    /// Zigzag along the bottom edge.
    Squiggly,
    /// Straight line through the vertical middle.
    StrikeOut,
}

impl MarkupLine {
    /// Fraction of the way from a quad's top edge to its bottom edge.
    fn ratio(self) -> f32 {
        match self {
            Self::Underline | Self::Squiggly => 1.0,
            Self::StrikeOut => 0.5,
        }
    }
}

impl Highlight {
    /// One translucent filled quadrilateral per selection region, in local units of `bounds`.
    pub(crate) fn shapes(&self, bounds: &Rect, style: &ResolvedStyle) -> Vec<Shape> {
        let paint = ShapePaint {
            stroke: None,
            fill: Some(style.color.with_alpha(HIGHLIGHT_ALPHA)),
            width: 0.0,
            dash: Vec::new(),
            round: false,
        };
        self.quads
            .iter()
            .map(|quad| Shape::Polyline {
                path: Polyline::from(quad).map(|corner| bounds.to_local(corner)),
                paint: paint.clone(),
            })
            .collect()
    }
}

impl Markup {
    /// One `line` per quad, in local units of `bounds`.
    pub(crate) fn shapes(
        &self,
        bounds: &Rect,
        style: &ResolvedStyle,
        line: MarkupLine,
    ) -> Vec<Shape> {
        let ratio = line.ratio();
        let paint = style.outline(false);
        self.quads
            .iter()
            .map(|quad| {
                let [tl, tr, br, bl] = quad.corners.map(|corner| bounds.to_local(corner));
                let (a, b) = (tl.lerp(bl, ratio), tr.lerp(br, ratio));
                let path = match line {
                    MarkupLine::Squiggly => Polyline::squiggle(a, b),
                    MarkupLine::Underline | MarkupLine::StrikeOut => {
                        Polyline::new(vec![a, b], false)
                    }
                };
                Shape::Polyline {
                    path,
                    paint: paint.clone(),
                }
            })
            .collect()
    }
}
