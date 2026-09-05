//! Single-pass PDF glyph fallback, positioning, and run construction.
//!
//! Decoded glyphs are resolved and appended directly to their final face-homogeneous runs. This
//! avoids a pipeline of cloned intermediate records while keeping PDF positioning, global font
//! metrics, and fallback policy as distinct steps.

use std::collections::HashMap;
use std::sync::Arc;

use pdf_graphics::{rect::Rect, transform::Transform};

use crate::error::TextError;
use crate::pdf_text_decoder::{DecodedGlyph, GlyphSelector, decode_pdf_text};
use crate::system::FontSystem;
use crate::text::{GlyphRun, PdfTextItem, PdfTextRun, PositionedGlyph, TextLayout, TextVector};
use crate::text_style::TextStyle;
use pdf_cmap::{Cid, PdfCode, WritingMode};
use pdf_font::pdf_font_handle::PdfFontHandle;
use pdf_font::{
    FontFace, FontFaceId, FontMetrics, GlyphFallbackRequest, GlyphId, PDF_GLYPH_SPACE_UNITS_PER_EM,
    PdfFontSpec,
    font::{FALLBACK_ASCENDER_EM_RATIO, FALLBACK_DESCENDER_EM_RATIO},
};

/// Per-layout memoization of glyph fallback results, including definitive misses.
///
/// The Unicode scalar is sufficient as the key because a cache belongs to exactly one layout and
/// therefore one requested PDF font. Caching `None` prevents repeatedly querying providers for a
/// character they cannot supply.
type FallbackCache = HashMap<char, Option<(Arc<dyn FontFace>, GlyphId)>>;

/// Resolves and positions one PDF text run while retaining only final render data.
///
/// Text strings and `TJ` adjustments are consumed in their original order. Each decoded code is
/// immediately resolved to a face, transformed, bounded, and appended to a run; no decoded-glyph
/// collection survives between those stages.
pub(crate) fn layout_pdf(
    system: &FontSystem,
    run: &PdfTextRun<'_>,
) -> Result<TextLayout, TextError> {
    let mut runs: Vec<GlyphRun> = Vec::new();
    let advance = visit_pdf(system, run, |face, glyph| {
        if let Some(glyph_run) = runs
            .last_mut()
            .filter(|glyph_run| glyph_run.face.id() == face.id())
        {
            glyph_run.glyphs.push(glyph);
        } else {
            runs.push(GlyphRun {
                face: Arc::clone(face),
                glyphs: vec![glyph],
            });
        }
        Ok::<(), TextError>(())
    })?;
    Ok(TextLayout { runs, advance })
}

/// Resolves and positions a PDF text run while emitting each glyph immediately.
pub(crate) fn visit_pdf<E>(
    system: &FontSystem,
    run: &PdfTextRun<'_>,
    visitor: impl FnMut(&Arc<dyn FontFace>, PositionedGlyph) -> Result<(), E>,
) -> Result<TextVector, E>
where
    E: From<TextError>,
{
    let mut layouter = PdfTextLayouter::new(system, run.font, &run.style, visitor);
    for item in run.items {
        match item {
            PdfTextItem::Adjustment(amount) => layouter.adjust(*amount),
            PdfTextItem::Text(bytes) => {
                decode_pdf_text(run.font.spec(), bytes, |decoded| layouter.push(decoded))?;
            }
        }
    }
    Ok(layouter.finish())
}

/// Mutable state for the single pass over a [`PdfTextRun`].
struct PdfTextLayouter<'a, V> {
    /// Font services used for on-demand fallback loading.
    system: &'a FontSystem,
    /// Loaded PDF font, including its normalized specification and primary face.
    font: &'a PdfFontHandle,
    /// PDF text state applied to every item in this input run.
    style: &'a TextStyle,
    /// Direction controlling advances and `TJ` adjustments.
    writing_mode: WritingMode,
    /// Current baseline position relative to the layout origin.
    pen: TextVector,
    /// Consumer receiving each final positioned glyph.
    visitor: V,
    /// Metrics cached for the most recently resolved face.
    ///
    /// Glyphs normally remain on one face for long stretches, so a single-entry cache avoids a
    /// virtual metrics call without retaining a second face map.
    current_metrics: Option<(FontFaceId, Option<FontMetrics>)>,
    /// Glyph fallback decisions scoped to this layout operation.
    fallback_cache: FallbackCache,
    /// Resolution decisions for repeated PDF source codes in this text operation.
    resolved_cache: HashMap<PdfCode, ResolvedGlyph>,
}

impl<'a, V, E> PdfTextLayouter<'a, V>
where
    V: FnMut(&Arc<dyn FontFace>, PositionedGlyph) -> Result<(), E>,
    E: From<TextError>,
{
    /// Starts a layout at the origin using the font's normalized writing mode.
    fn new(
        system: &'a FontSystem,
        font: &'a PdfFontHandle,
        style: &'a TextStyle,
        visitor: V,
    ) -> Self {
        Self {
            system,
            font,
            style,
            writing_mode: font.spec().writing_mode(),
            pen: TextVector::default(),
            visitor,
            current_metrics: None,
            fallback_cache: HashMap::new(),
            resolved_cache: HashMap::new(),
        }
    }

    /// Applies one numeric `TJ` adjustment to the current pen.
    ///
    /// PDF adjustments have the opposite sign from pen movement and are expressed in thousandths
    /// of text space. Horizontal scaling affects horizontal writing only.
    fn adjust(&mut self, amount: f32) {
        let adjustment = -amount / PDF_GLYPH_SPACE_UNITS_PER_EM * self.style.font_size;
        match self.writing_mode {
            WritingMode::Horizontal => {
                self.pen.x += adjustment * self.style.horizontal_scale;
            }
            WritingMode::Vertical => self.pen.y += adjustment,
        }
    }

    /// Resolves, positions, and appends one decoded glyph, then advances the pen.
    ///
    /// PDF widths remain authoritative for native faces. Horizontal glyphs supplied by an
    /// unrelated fallback face use that face's advance so its outlines retain their intended
    /// spacing, falling back to the PDF width when the substitute does not contain the glyph.
    fn push(&mut self, decoded: DecodedGlyph<'_>) -> Result<(), E> {
        let resolved = if let Some(cached) = self.resolved_cache.get(&decoded.source_code) {
            cached.clone()
        } else {
            let resolved =
                resolve_glyph(self.system, self.font, &mut self.fallback_cache, &decoded)
                    .map_err(E::from)?;
            self.resolved_cache
                .insert(decoded.source_code, resolved.clone());
            resolved
        };
        let face = resolved.face(self.font);
        let face_id = face.id();
        let face_metrics = match self.current_metrics {
            Some((cached_id, metrics)) if cached_id == face_id => metrics,
            _ => {
                let metrics = face.metrics();
                self.current_metrics = Some((face_id, metrics));
                metrics
            }
        };

        let needs_fallback_advance =
            resolved.uses_fallback_face(self.font) && self.writing_mode == WritingMode::Horizontal;
        let natural = if needs_fallback_advance {
            match face.horizontal_advance(resolved.glyph_id) {
                Ok(Some(advance)) => TextVector {
                    x: advance / units_per_em(face_metrics),
                    y: 0.0,
                },
                Ok(None) | Err(pdf_font::FontError::MissingGlyph { .. }) => TextVector {
                    x: decoded.pdf_advance.x / PDF_GLYPH_SPACE_UNITS_PER_EM,
                    y: decoded.pdf_advance.y / PDF_GLYPH_SPACE_UNITS_PER_EM,
                },
                Err(error) => return Err(E::from(TextError::from(error))),
            }
        } else {
            TextVector {
                x: decoded.pdf_advance.x / PDF_GLYPH_SPACE_UNITS_PER_EM,
                y: decoded.pdf_advance.y / PDF_GLYPH_SPACE_UNITS_PER_EM,
            }
        };
        let advance = styled_advance(&decoded, natural, self.style, self.writing_mode);
        let transform = glyph_transform(self.font.spec(), face_metrics, self.style, self.pen);
        let bounds = glyph_bounds(
            face_metrics,
            self.style,
            self.pen,
            advance,
            self.writing_mode,
        );
        let glyph = PositionedGlyph {
            glyph_id: resolved.glyph_id,
            local_transform: transform,
            bounds,
            unicode: decoded.unicode,
        };

        (self.visitor)(resolved.face_arc(self.font), glyph)?;
        self.pen.x += advance.x;
        self.pen.y += advance.y;
        Ok(())
    }

    /// Converts the accumulated state into the backend-independent layout result.
    fn finish(self) -> TextVector {
        self.pen
    }
}

/// Face ownership chosen for one resolved glyph.
///
/// Primary glyphs use a marker rather than cloning the primary `Arc` for every character. A
/// fallback face must be owned because it is loaded during layout and must outlive the resolver.
#[derive(Clone)]
enum ResolvedFace {
    /// The primary face already owned by the [`PdfFontHandle`].
    Primary,
    /// A dynamically loaded face that supplies a missing Unicode character.
    Fallback(Arc<dyn FontFace>),
}

/// Physical face and glyph ID selected for one decoded PDF code.
#[derive(Clone)]
struct ResolvedGlyph {
    /// Whether the glyph uses the handle's primary face or an owned fallback face.
    face: ResolvedFace,
    /// Face-local glyph identifier.
    glyph_id: GlyphId,
}

impl ResolvedGlyph {
    /// Borrows the selected face without changing shared ownership counts.
    fn face<'a>(&'a self, font: &'a PdfFontHandle) -> &'a dyn FontFace {
        match &self.face {
            ResolvedFace::Primary => font.primary().as_ref(),
            ResolvedFace::Fallback(face) => face.as_ref(),
        }
    }

    /// Returns the shared face selected for this glyph.
    fn face_arc<'a>(&'a self, font: &'a PdfFontHandle) -> &'a Arc<dyn FontFace> {
        match &self.face {
            ResolvedFace::Primary => font.primary(),
            ResolvedFace::Fallback(face) => face,
        }
    }

    /// Returns whether this glyph comes from a face unrelated to the PDF font program.
    const fn uses_fallback_face(&self, font: &PdfFontHandle) -> bool {
        font.uses_substitute() || matches!(self.face, ResolvedFace::Fallback(_))
    }
}

/// Resolves a decoded selector against the primary face and then the fallback provider.
///
/// Explicit selectors and Unicode lookup are attempted on the primary face first. If no glyph is
/// found, the first Unicode scalar drives per-glyph fallback; multi-scalar mappings still describe
/// one PDF glyph and therefore do not request several faces. Missing Unicode or exhausted fallback
/// resolves to glyph zero (`.notdef`) on the primary face.
fn resolve_glyph(
    system: &FontSystem,
    font: &PdfFontHandle,
    cache: &mut FallbackCache,
    decoded: &DecodedGlyph<'_>,
) -> Result<ResolvedGlyph, TextError> {
    if let Some(glyph_id) = select_glyph(
        font.primary().as_ref(),
        &decoded.selector,
        decoded.unicode.as_slice(),
        font.uses_substitute(),
    ) {
        return Ok(ResolvedGlyph {
            face: ResolvedFace::Primary,
            glyph_id,
        });
    }

    let Some(character) = decoded.unicode.as_slice().first().copied() else {
        return Ok(ResolvedGlyph {
            face: ResolvedFace::Primary,
            glyph_id: GlyphId(0),
        });
    };
    if let Some(cached) = cache.get(&character) {
        return Ok(match cached {
            Some((face, glyph_id)) => ResolvedGlyph {
                face: ResolvedFace::Fallback(Arc::clone(face)),
                glyph_id: *glyph_id,
            },
            None => ResolvedGlyph {
                face: ResolvedFace::Primary,
                glyph_id: GlyphId(0),
            },
        });
    }

    let excluded = [font.primary().id()];
    let candidates = system
        .fallback_provider()
        .glyph_candidates(&GlyphFallbackRequest {
            character,
            requested: font.primary().metadata(),
            pdf_font: Some(font.spec()),
            excluded_faces: &excluded,
        })?;
    for candidate in candidates {
        let Ok(face) = system.load_with_metadata(candidate.source, candidate.metadata) else {
            continue;
        };
        if let Some(glyph_id) = face.glyph_for_char(character) {
            cache.insert(character, Some((Arc::clone(&face), glyph_id)));
            return Ok(ResolvedGlyph {
                face: ResolvedFace::Fallback(face),
                glyph_id,
            });
        }
    }
    cache.insert(character, None);
    Ok(ResolvedGlyph {
        face: ResolvedFace::Primary,
        glyph_id: GlyphId(0),
    })
}

/// Selects a glyph already available in `face` without consulting fallback services.
///
/// Whole-font substitutes prefer Unicode because embedded glyph IDs, names, and CIDs generally do
/// not address the substitute's glyph space. Native faces try their explicit PDF selector first,
/// then use the first Unicode scalar as a recovery path.
fn select_glyph(
    face: &dyn FontFace,
    selector: &GlyphSelector<'_>,
    unicode: &[char],
    prefer_unicode: bool,
) -> Option<GlyphId> {
    let unicode_glyph = || {
        unicode
            .first()
            .and_then(|character| face.glyph_for_char(*character))
    };
    if prefer_unicode && let Some(glyph) = unicode_glyph() {
        return Some(glyph);
    }
    let selected = match selector {
        GlyphSelector::GlyphId(glyph) => Some(*glyph),
        GlyphSelector::GlyphName(name) => face.glyph_for_name(name),
        GlyphSelector::Cid(Cid(cid)) => Some(GlyphId(*cid)),
        GlyphSelector::Unspecified => None,
    };
    selected.or_else(unicode_glyph)
}

/// Returns a safe scale denominator for physical font metrics.
///
/// [`PDF_GLYPH_SPACE_UNITS_PER_EM`] is used when a face omits metrics or reports an invalid zero.
fn units_per_em(metrics: Option<FontMetrics>) -> f32 {
    metrics
        .map(|value| value.units_per_em)
        .filter(|value| *value != 0)
        .map(f32::from)
        .unwrap_or(PDF_GLYPH_SPACE_UNITS_PER_EM)
}

/// Applies PDF font size, spacing, writing direction, and horizontal scale to an advance.
///
/// Word spacing applies only to the single-byte source code `0x20`, as required by PDF text-state
/// semantics; it is not inferred from the Unicode mapping.
fn styled_advance(
    decoded: &DecodedGlyph<'_>,
    natural: TextVector,
    style: &TextStyle,
    writing_mode: WritingMode,
) -> TextVector {
    let word_spacing = if decoded.source_code.value() == 0x20 {
        style.word_spacing
    } else {
        0.0
    };
    match writing_mode {
        WritingMode::Horizontal => TextVector {
            x: (natural.x * style.font_size + style.character_spacing + word_spacing)
                * style.horizontal_scale,
            y: natural.y * style.font_size,
        },
        WritingMode::Vertical => TextVector {
            x: natural.x * style.font_size,
            y: natural.y * style.font_size + style.character_spacing + word_spacing,
        },
    }
}

/// Builds the glyph-to-layout transform at `origin`.
///
/// Type 3 glyphs begin in the PDF-provided font matrix. Other faces begin with a units-per-em
/// normalization. Font size, horizontal scaling, pen position, and text rise are then composed in
/// layout coordinates.
fn glyph_transform(
    spec: &PdfFontSpec,
    metrics: Option<FontMetrics>,
    style: &TextStyle,
    origin: TextVector,
) -> Transform {
    let mut transform = match spec {
        PdfFontSpec::Type3(font) => font.font_matrix,
        _ => {
            let units = units_per_em(metrics);
            Transform::from_scale(1.0 / units, 1.0 / units)
        }
    };
    transform.scale(style.font_size * style.horizontal_scale, style.font_size);
    transform.translate(origin.x, origin.y + style.rise);
    transform
}

/// Computes a conservative layout-space cell for one positioned glyph.
///
/// PDF advance determines the cell extent along the writing direction. Horizontal cells use the
/// face ascender and descender for their vertical extent, with conventional proportions when the
/// physical face has no global metrics.
fn glyph_bounds(
    face_metrics: Option<FontMetrics>,
    style: &TextStyle,
    origin: TextVector,
    advance: TextVector,
    writing_mode: WritingMode,
) -> Rect {
    let (ascender, descender) = face_metrics
        .and_then(|metrics| {
            let units = f32::from(metrics.units_per_em.max(1));
            let ascender = metrics.ascender / units * style.font_size;
            let descender = metrics.descender / units * style.font_size;
            (ascender.is_finite() && descender.is_finite() && ascender != descender)
                .then_some((ascender, descender))
        })
        .unwrap_or((
            style.font_size * FALLBACK_ASCENDER_EM_RATIO,
            style.font_size * FALLBACK_DESCENDER_EM_RATIO,
        ));
    let baseline = origin.y + style.rise;
    match writing_mode {
        WritingMode::Horizontal => Rect {
            left: origin.x,
            top: baseline + descender,
            right: origin.x + advance.x,
            bottom: baseline + ascender,
        }
        .normalized(),
        WritingMode::Vertical => Rect {
            left: origin.x,
            top: origin.y,
            right: origin.x + style.font_size * style.horizontal_scale,
            bottom: origin.y + advance.y,
        }
        .normalized(),
    }
}
