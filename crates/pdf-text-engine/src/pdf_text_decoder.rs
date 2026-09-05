//! Allocation-free decoding of PDF text strings into logical glyph inputs.
//!
//! This module deliberately stops before selecting a physical font face. PDF character codes can
//! identify a glyph by glyph ID, glyph name, or CID, and can independently map to one or more
//! Unicode scalars. Keeping those concerns separate lets layout choose a primary or fallback face
//! without allocating an intermediate decoded-text collection.

use crate::error::TextError;
use crate::text::TextVector;
use pdf_cmap::{Cid, CidMapping, PdfCode, ToUnicodeMap, UnicodeSequence};
use pdf_font::{
    CidFontKind, CidFontSpec, GlyphId, GlyphName, PdfFontSpec, PdfGlyphMetric, PdfMetrics,
    SimpleEncoding, Type0FontSpec,
};

/// Borrowed glyph-selection data valid only while its font specification is borrowed.
pub(crate) enum GlyphSelector<'a> {
    /// A glyph ID supplied by a CID-to-GID map.
    GlyphId(GlyphId),
    /// A glyph name borrowed from a simple or Type 3 font encoding difference.
    GlyphName(&'a GlyphName),
    /// A CID to interpret according to the selected descendant font.
    Cid(Cid),
    /// No explicit selector was available; layout may still select by Unicode.
    Unspecified,
}

/// One decoded PDF character code ready for face selection and positioning.
pub(crate) struct DecodedGlyph<'a> {
    /// Original variable-width PDF code, retained for word-spacing semantics.
    pub(crate) source_code: PdfCode,
    /// Font-specific selector used before falling back to Unicode lookup.
    pub(crate) selector: GlyphSelector<'a>,
    /// Text-extraction mapping associated with the source code.
    pub(crate) unicode: UnicodeSequence,
    /// PDF-specified advance in thousandths of a text-space unit.
    pub(crate) pdf_advance: TextVector,
}

/// Decodes `bytes` and immediately passes each logical glyph to `consume`.
///
/// A visitor keeps borrowed glyph names usable without boxing a heterogeneous iterator and avoids
/// materializing a second collection after a CMap has decoded the source bytes. Type 0 fonts are
/// decoded one variable-width CMap entry at a time; simple and Type 3 fonts consume one byte per
/// entry. Unicode comes from `/ToUnicode` first, then the descendant CID map or the source byte as
/// appropriate for that font kind.
///
/// The visitor is called in source order. Its error is returned immediately, so callers never
/// observe later characters after a layout or font-resolution failure.
pub(crate) fn decode_pdf_text<'a, E>(
    spec: &'a PdfFontSpec,
    bytes: &[u8],
    mut consume: impl FnMut(DecodedGlyph<'a>) -> Result<(), E>,
) -> Result<(), E>
where
    E: From<TextError>,
{
    match spec {
        PdfFontSpec::Type0(font) => decode_type0(font, bytes, &mut consume),
        PdfFontSpec::Type1(font)
        | PdfFontSpec::MultipleMasterType1(font)
        | PdfFontSpec::TrueType(font) => decode_one_byte_font(
            &font.encoding,
            &font.metrics,
            font.to_unicode.as_deref(),
            bytes,
            &mut consume,
        ),
        PdfFontSpec::Type3(font) => decode_one_byte_font(
            &font.encoding,
            &font.metrics,
            font.to_unicode.as_deref(),
            bytes,
            &mut consume,
        ),
    }
}

/// Streams a composite-font string through its variable-width encoding CMap.
///
/// This function owns only iteration and forward-progress validation. Conversion of an individual
/// CMap result into a logical glyph is delegated to [`decode_type0_mapping`].
fn decode_type0<'a, E>(
    font: &'a Type0FontSpec,
    bytes: &[u8],
    consume: &mut impl FnMut(DecodedGlyph<'a>) -> Result<(), E>,
) -> Result<(), E>
where
    E: From<TextError>,
{
    let mut remaining = bytes;
    while let Some(mapping) = font
        .encoding
        .decode_next(remaining)
        .map_err(TextError::from)
        .map_err(E::from)?
    {
        consume(decode_type0_mapping(font, mapping))?;
        remaining = advance_type0_input(bytes, remaining, mapping.source).map_err(E::from)?;
    }
    Ok(())
}

/// Converts one CMap result into the selector, Unicode mapping, and PDF advance used by layout.
fn decode_type0_mapping(font: &Type0FontSpec, mapping: CidMapping) -> DecodedGlyph<'static> {
    DecodedGlyph {
        source_code: mapping.source,
        selector: type0_selector(&font.descendant, mapping.cid),
        unicode: type0_unicode(font, mapping),
        pdf_advance: metric_advance(metric_for(&font.descendant.metrics, mapping.cid.0)),
    }
}

/// Selects the descendant glyph identifier associated with a CID.
///
/// CIDFontType0 uses the CID directly. CIDFontType2 first consults its optional CID-to-GID table;
/// a missing, out-of-range, or non-`usize` CID remains a CID so later face selection can recover by
/// Unicode or use the conventional identity mapping.
fn type0_selector(descendant: &CidFontSpec, cid: Cid) -> GlyphSelector<'static> {
    if descendant.kind == CidFontKind::Type0 {
        return GlyphSelector::Cid(cid);
    }

    let Some(map) = descendant.cid_to_gid.as_deref() else {
        return GlyphSelector::Cid(cid);
    };
    let Ok(index) = usize::try_from(cid.0) else {
        return GlyphSelector::Cid(cid);
    };
    map.get(index)
        .copied()
        .map(|glyph| GlyphSelector::GlyphId(GlyphId(u32::from(glyph))))
        .unwrap_or(GlyphSelector::Cid(cid))
}

/// Resolves extraction Unicode for one composite-font mapping.
///
/// The source-code `/ToUnicode` map is authoritative. A best-effort collection CID map is used only
/// when `/ToUnicode` has no entry and the CID fits that map's 16-bit key space.
fn type0_unicode(font: &Type0FontSpec, mapping: CidMapping) -> UnicodeSequence {
    font.to_unicode
        .as_deref()
        .and_then(|map| map.map(mapping.source))
        .or_else(|| {
            let cid = u16::try_from(mapping.cid.0).ok()?;
            font.descendant
                .cid_to_unicode
                .as_ref()?
                .get(&cid)
                .copied()
                .map(Into::into)
        })
        .unwrap_or_default()
}

/// Advances past one streamed CMap result and rejects an invalid consumed length.
///
/// [`crate::pdf::PdfCMap`] requires forward progress, but this boundary still validates custom
/// implementations before slicing their untrusted byte count from the source.
fn advance_type0_input<'a>(
    original: &[u8],
    remaining: &'a [u8],
    source: PdfCode,
) -> Result<&'a [u8], TextError> {
    remaining
        .get(usize::from(source.byte_len())..)
        .ok_or_else(|| TextError::InvalidCharacterCode {
            offset: original.len().saturating_sub(remaining.len()),
        })
}

/// Decodes the one-byte representation shared by simple and Type 3 PDF fonts.
///
/// Encoding names remain borrowed from `encoding`. Widths come from the explicit PDF table when
/// present and otherwise from its normalized default. The source byte is used as Unicode only when
/// no `/ToUnicode` entry exists.
fn decode_one_byte_font<'a, E>(
    encoding: &'a SimpleEncoding,
    metrics: &PdfMetrics,
    to_unicode: Option<&dyn ToUnicodeMap>,
    bytes: &[u8],
    consume: &mut impl FnMut(DecodedGlyph<'a>) -> Result<(), E>,
) -> Result<(), E>
where
    E: From<TextError>,
{
    bytes.iter().copied().try_for_each(|code| {
        consume(decode_one_byte_mapping(encoding, metrics, to_unicode, code).map_err(E::from)?)
    })
}

/// Converts one byte into its borrowed name selector, Unicode mapping, and PDF advance.
fn decode_one_byte_mapping<'a>(
    encoding: &'a SimpleEncoding,
    metrics: &PdfMetrics,
    to_unicode: Option<&dyn ToUnicodeMap>,
    code: u8,
) -> Result<DecodedGlyph<'a>, TextError> {
    let source = PdfCode::new(u32::from(code), 1)?;
    Ok(DecodedGlyph {
        source_code: source,
        selector: encoding
            .differences
            .get(&code)
            .map(GlyphSelector::GlyphName)
            .unwrap_or(GlyphSelector::Unspecified),
        unicode: to_unicode
            .and_then(|map| map.map(source))
            .or_else(|| {
                encoding
                    .differences
                    .get(&code)
                    .and_then(|name| {
                        pdf_font::glyph_name_to_unicode::glyph_name_to_unicode(name.0.as_ref())
                    })
                    .map(Into::into)
            })
            .unwrap_or_else(|| char::from(code).into()),
        pdf_advance: metric_advance(metric_for(metrics, u32::from(code))),
    })
}

/// Returns an explicit PDF metric or the table's normalized default.
fn metric_for(metrics: &PdfMetrics, code: u32) -> PdfGlyphMetric {
    metrics
        .explicit
        .get(&code)
        .copied()
        .unwrap_or(metrics.default)
}

/// Retains only the advance components needed by the layout stage.
const fn metric_advance(metric: PdfGlyphMetric) -> TextVector {
    TextVector {
        x: metric.advance_x,
        y: metric.advance_y,
    }
}
