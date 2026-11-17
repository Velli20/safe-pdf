//! Font programs a host lays Unicode text out with, such as a browser `FontFace`, built
//! from the program a PDF font draws with.
//!
//! A PDF selects a simple font's glyphs by character code, through encodings and cmap
//! subtables a host resolving Unicode text cannot use, and spaces them by the PDF's own
//! widths. The host program keeps the drawing program's outlines and gives it the tables
//! a host needs: a Unicode `cmap` onto the glyphs the PDF draws, horizontal metrics with
//! the PDF widths, and the naming and metrics tables browsers require.

use read_fonts::{FontRef, TableProvider, ps::cff::CffFontRef};
use std::collections::BTreeMap;
use write_fonts::{
    BuilderError, FontBuilder, OffsetMarker,
    from_obj::ToOwnedTable,
    tables::{
        cmap::Cmap,
        gpos::{Gpos, PositionLookup, PositionLookupList, SinglePos, ValueRecord},
        head::Head,
        hhea::Hhea,
        hmtx::{Hmtx, LongMetric},
        layout::{
            CoverageTable, Feature, FeatureList, FeatureRecord, LangSys, Lookup, LookupFlag,
            Script, ScriptList, ScriptRecord,
        },
        maxp::Maxp,
        name::{Name, NameRecord},
        os2::Os2,
        post::Post,
    },
    types::{FWord, GlyphId16, NameId, Tag, UfWord, Version16Dot16},
};

use crate::error::FontError;
use crate::font::{FontProgramFormat, FontSource, GlyphId};
use crate::pdf_font_handle::LineMetrics;

/// Tables the host program rebuilds, or leaves out, rather than copies from the drawing
/// program. PDF text is drawn without the program's kerning and glyph substitutions, so
/// a host must not apply them either.
const REBUILT_TABLES: [Tag; 10] = [
    Tag::new(b"cmap"),
    Tag::new(b"hhea"),
    Tag::new(b"hmtx"),
    Tag::new(b"name"),
    Tag::new(b"OS/2"),
    Tag::new(b"post"),
    Tag::new(b"DSIG"),
    Tag::new(b"GPOS"),
    Tag::new(b"GSUB"),
    Tag::new(b"kern"),
];
/// The feature a host applies by default that centres comb glyphs in their cells.
const KERN: Tag = Tag::new(b"kern");
/// Scripts the comb centring feature is registered for, in tag order.
const COMB_SCRIPTS: [Tag; 2] = [Tag::new(b"DFLT"), Tag::new(b"latn")];
/// Tag of a bare CFF program's table in an OpenType container.
const CFF: Tag = Tag::new(b"CFF ");
/// Units per em of PDF glyph-space widths.
const PDF_GLYPH_SPACE_UNITS: f32 = 1000.0;
/// Windows platform identifier of the name records.
const WINDOWS_PLATFORM: u16 = 3;
/// Unicode BMP encoding identifier of the name records.
const UNICODE_BMP_ENCODING: u16 = 1;
/// US English language identifier of the name records.
const ENGLISH_LANGUAGE: u16 = 0x409;
/// `OS/2` code page bit for Latin 1, which also selects table version 1.
const LATIN_1_CODE_PAGE: u32 = 1;

/// One character a host program maps, with the glyph the PDF draws for it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HostGlyph {
    /// The Unicode character.
    pub character: char,
    /// The glyph the PDF draws for the character.
    pub glyph: GlyphId,
    /// The PDF advance, in thousandths of an em.
    pub advance: f32,
}

/// A font program a host lays Unicode text out with: each mapped character shows the
/// glyph the PDF draws for it and advances by the PDF width.
pub struct HostFontProgram {
    /// The program bytes.
    data: Vec<u8>,
    /// The distance between baselines the program's line metrics give, in ems.
    line_height: f32,
}

impl HostFontProgram {
    /// Builds the host program named `family` from `source`, mapping `glyphs`, with the
    /// ascender and descender of `line` and no line gap, so a host lays lines out as the
    /// PDF text drawn in the font is.
    ///
    /// With a `comb_cell` width, in thousandths of an em, every mapped glyph advances by
    /// the cell and is positioned in the middle of it, as a comb field's characters are
    /// drawn one per cell.
    ///
    /// Returns `None` for a source this cannot rebuild: a Type 1, CID-keyed CFF, Type 3 or
    /// external program, or one whose tables are unreadable.
    ///
    /// # Errors
    /// Returns [`FontError::FontBuildError`] when a rebuilt table cannot be written.
    pub fn new(
        source: &FontSource,
        family: &str,
        glyphs: &[HostGlyph],
        line: LineMetrics,
        comb_cell: Option<f32>,
    ) -> Result<Option<Self>, FontError> {
        let FontSource::Memory {
            data,
            format,
            face_index,
        } = source
        else {
            return Ok(None);
        };
        let Some(program) = DrawingProgram::read(data, *format, *face_index) else {
            return Ok(None);
        };
        program
            .build(family, glyphs, line, comb_cell)
            .map(|data| {
                Some(Self {
                    data,
                    line_height: line.line_height(),
                })
            })
            .map_err(|error| FontError::FontBuildError(error.to_string()))
    }

    /// Borrows the program bytes.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// The distance between baselines, in ems, of lines laid out in the program.
    #[must_use]
    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    /// Takes the program bytes.
    #[must_use]
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }
}

/// The drawing program the host program is built from.
enum DrawingProgram<'a> {
    /// An OpenType or TrueType container, whose outline and hinting tables are kept.
    Sfnt(FontRef<'a>),
    /// A bare name-keyed CFF program, wrapped in an OpenType container.
    Cff {
        /// The program bytes, which become the container's `CFF ` table.
        data: &'a [u8],
        /// The parsed program.
        font: CffFontRef<'a>,
    },
}

impl<'a> DrawingProgram<'a> {
    /// Reads `data` declared as `format`; `None` for a format or program this cannot rebuild.
    fn read(data: &'a [u8], format: FontProgramFormat, face_index: u32) -> Option<Self> {
        match format {
            FontProgramFormat::TrueType
            | FontProgramFormat::OpenTypeTrueType
            | FontProgramFormat::OpenTypeCff => {
                FontRef::from_index(data, face_index).ok().map(Self::Sfnt)
            }
            FontProgramFormat::Cff => CffFontRef::new_cff(data, 0, None)
                .ok()
                .filter(|font| !font.is_cid())
                .map(|font| Self::Cff { data, font }),
            _ => None,
        }
    }

    /// Units per em of the program's design space.
    fn units_per_em(&self) -> Option<u16> {
        match self {
            Self::Sfnt(font) => font.head().ok().map(|head| head.units_per_em()),
            Self::Cff { font, .. } => u16::try_from(font.upem()).ok(),
        }
        .filter(|units| *units != 0)
    }

    /// The number of glyphs in the program.
    fn num_glyphs(&self) -> Option<u16> {
        match self {
            Self::Sfnt(font) => font.maxp().ok().map(|maxp| maxp.num_glyphs()),
            Self::Cff { font, .. } => u16::try_from(font.num_glyphs()).ok(),
        }
    }

    /// The program's font bounds as `[x_min, y_min, x_max, y_max]` in design units.
    fn bounds(&self) -> [i16; 4] {
        match self {
            Self::Sfnt(font) => font.head().map_or([0; 4], |head| {
                [head.x_min(), head.y_min(), head.x_max(), head.y_max()]
            }),
            Self::Cff { font, .. } => font.metadata().map_or([0; 4], |metadata| {
                let bbox = metadata.bbox();
                [bbox.x_min, bbox.y_min, bbox.x_max, bbox.y_max]
                    .map(|value| saturating_i16(value.to_f32()))
            }),
        }
    }

    /// Every glyph's advance and left side bearing in the program, in design units; zero
    /// for a program without horizontal metrics.
    fn metrics(&self, num_glyphs: u16) -> Vec<LongMetric> {
        let mut metrics = vec![LongMetric::default(); usize::from(num_glyphs)];
        if let Self::Sfnt(font) = self
            && let Ok(hmtx) = font.hmtx()
        {
            let long = hmtx.h_metrics();
            let mut last = LongMetric::default();
            for (index, metric) in metrics.iter_mut().enumerate() {
                if let Some(long) = long.get(index) {
                    last = LongMetric::new(long.advance(), long.side_bearing());
                    *metric = last.clone();
                } else {
                    let bearing = index
                        .checked_sub(long.len())
                        .and_then(|offset| hmtx.left_side_bearings().get(offset))
                        .map_or(0, |bearing| bearing.get());
                    *metric = LongMetric::new(last.advance, bearing);
                }
            }
        }
        metrics
    }

    /// Builds the host program.
    fn build(
        &self,
        family: &str,
        glyphs: &[HostGlyph],
        line: LineMetrics,
        comb_cell: Option<f32>,
    ) -> Result<Vec<u8>, HostBuildError> {
        let units_per_em = self.units_per_em().ok_or(HostBuildError::Unreadable)?;
        let num_glyphs = self.num_glyphs().ok_or(HostBuildError::Unreadable)?;
        let scale = f32::from(units_per_em) / PDF_GLYPH_SPACE_UNITS;
        let mut metrics = self.metrics(num_glyphs);
        for glyph in glyphs {
            if let Some(metric) = usize::try_from(glyph.glyph.0)
                .ok()
                .and_then(|index| metrics.get_mut(index))
            {
                metric.advance = saturating_u16(comb_cell.unwrap_or(glyph.advance) * scale);
            }
        }
        let [x_min, y_min, x_max, y_max] = self.bounds();
        let vertical = VerticalMetrics::new(line, units_per_em);
        let mut builder = FontBuilder::new();
        builder
            .add_table(&host_cmap(glyphs, num_glyphs)?)?
            .add_table(&self.hhea(&metrics, vertical))?
            .add_table(&Hmtx::new(metrics.clone(), Vec::new()))?
            .add_table(&host_name(family))?
            .add_table(&host_os2(glyphs, &metrics, vertical))?
            .add_table(&Post {
                version: Version16Dot16::VERSION_3_0,
                ..Post::default()
            })?;
        if let Some(cell) = comb_cell {
            builder.add_table(&comb_gpos(glyphs, cell, scale, num_glyphs))?;
        }
        match self {
            Self::Sfnt(font) => {
                // Outline, hinting and header tables are kept as they are; the digital
                // signature no longer matches and is dropped.
                for record in font.table_directory().table_records() {
                    let tag = record.tag();
                    if !REBUILT_TABLES.contains(&tag)
                        && let Some(data) = font.table_data(tag)
                    {
                        builder.add_raw(tag, data.as_bytes());
                    }
                }
            }
            Self::Cff { data, .. } => {
                builder
                    .add_table(&Head {
                        units_per_em,
                        x_min,
                        y_min,
                        x_max,
                        y_max,
                        ..Head::default()
                    })?
                    .add_table(&Maxp::new(num_glyphs))?
                    .add_raw(CFF, *data);
            }
        }
        Ok(builder.build())
    }

    /// The `hhea` table: the program's, when it has one, with the rebuilt metrics.
    fn hhea(&self, metrics: &[LongMetric], vertical: VerticalMetrics) -> Hhea {
        let base = match self {
            Self::Sfnt(font) => font.hhea().ok().map(|hhea| hhea.to_owned_table()),
            Self::Cff { .. } => None,
        };
        let advance_width_max = metrics.iter().map(|metric| metric.advance).max();
        Hhea {
            ascender: FWord::new(vertical.ascender),
            descender: FWord::new(vertical.descender),
            line_gap: FWord::new(0),
            advance_width_max: UfWord::new(advance_width_max.unwrap_or_default()),
            number_of_h_metrics: u16::try_from(metrics.len()).unwrap_or(u16::MAX),
            ..base.unwrap_or_else(|| Hhea {
                caret_slope_rise: 1,
                ..Hhea::default()
            })
        }
    }
}

/// The ascender and descender the host program lays lines out with.
#[derive(Clone, Copy)]
struct VerticalMetrics {
    /// Height above the baseline, in design units.
    ascender: i16,
    /// Depth below the baseline as a negative value, in design units.
    descender: i16,
}

impl VerticalMetrics {
    /// `line` in the design units of a program with `units_per_em`.
    fn new(line: LineMetrics, units_per_em: u16) -> Self {
        let units = f32::from(units_per_em);
        Self {
            ascender: saturating_i16(line.ascent * units),
            descender: saturating_i16(line.descent * units),
        }
    }
}

/// The `GPOS` table centring each mapped glyph in a comb cell `cell` thousandths of an em
/// wide, by the half of the cell its PDF advance leaves free, under the `kern` feature.
fn comb_gpos(glyphs: &[HostGlyph], cell: f32, scale: f32, num_glyphs: u16) -> Gpos {
    let placements: BTreeMap<u16, i16> = glyphs
        .iter()
        .filter_map(|glyph| {
            let id = u16::try_from(glyph.glyph.0)
                .ok()
                .filter(|id| *id < num_glyphs)?;
            Some((id, saturating_i16((cell - glyph.advance) * scale / 2.0)))
        })
        .collect();
    let coverage: CoverageTable = placements.keys().copied().map(GlyphId16::new).collect();
    let records = placements
        .values()
        .map(|placement| ValueRecord::new().with_x_placement(*placement))
        .collect();
    let lookup = PositionLookup::Single(Lookup::new(
        LookupFlag::empty(),
        vec![SinglePos::format_2(coverage, records)],
    ));
    let scripts = COMB_SCRIPTS
        .into_iter()
        .map(|tag| ScriptRecord::new(tag, Script::new(Some(LangSys::new(vec![0])), Vec::new())))
        .collect();
    Gpos::new(
        ScriptList::new(scripts),
        FeatureList::new(vec![FeatureRecord::new(KERN, Feature::new(None, vec![0]))]),
        PositionLookupList::new(vec![lookup]),
    )
}

/// The `cmap` mapping each character onto its glyph, leaving out glyphs the program lacks.
fn host_cmap(glyphs: &[HostGlyph], num_glyphs: u16) -> Result<Cmap, HostBuildError> {
    let mappings = glyphs
        .iter()
        .filter(|glyph| glyph.glyph.0 < u32::from(num_glyphs))
        .map(|glyph| {
            (
                glyph.character,
                write_fonts::types::GlyphId::new(glyph.glyph.0),
            )
        });
    Cmap::from_mappings(mappings).map_err(|conflict| HostBuildError::Table(conflict.to_string()))
}

/// The `name` table naming the program `family`.
fn host_name(family: &str) -> Name {
    let postscript: String = family
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '-')
        .collect();
    let record = |name_id, string: &str| {
        NameRecord::new(
            WINDOWS_PLATFORM,
            UNICODE_BMP_ENCODING,
            ENGLISH_LANGUAGE,
            name_id,
            OffsetMarker::new(string.to_owned()),
        )
    };
    Name::new(vec![
        record(NameId::FAMILY_NAME, family),
        record(NameId::SUBFAMILY_NAME, "Regular"),
        record(NameId::FULL_NAME, family),
        record(NameId::POSTSCRIPT_NAME, &postscript),
    ])
}

/// The `OS/2` table with the program's line metrics and mapped character range.
fn host_os2(glyphs: &[HostGlyph], metrics: &[LongMetric], vertical: VerticalMetrics) -> Os2 {
    let characters = || {
        glyphs
            .iter()
            .map(|glyph| u16::try_from(u32::from(glyph.character)).unwrap_or(u16::MAX))
    };
    let advances: Vec<u16> = metrics
        .iter()
        .map(|metric| metric.advance)
        .filter(|advance| *advance > 0)
        .collect();
    let average = advances
        .iter()
        .map(|advance| u32::from(*advance))
        .sum::<u32>()
        .checked_div(u32::try_from(advances.len()).unwrap_or(u32::MAX))
        .unwrap_or_default();
    let descent = vertical.descender.unsigned_abs();
    Os2 {
        x_avg_char_width: i16::try_from(average).unwrap_or(i16::MAX),
        us_first_char_index: characters().min().unwrap_or_default(),
        us_last_char_index: characters().max().unwrap_or_default(),
        s_typo_ascender: vertical.ascender,
        s_typo_descender: vertical.descender,
        us_win_ascent: vertical.ascender.unsigned_abs(),
        us_win_descent: descent,
        ul_code_page_range_1: Some(LATIN_1_CODE_PAGE),
        ul_code_page_range_2: Some(0),
        ..Os2::default()
    }
}

/// `value` rounded into the `i16` range.
fn saturating_i16(value: f32) -> i16 {
    num_traits::ToPrimitive::to_i16(
        &value
            .round()
            .clamp(f32::from(i16::MIN), f32::from(i16::MAX)),
    )
    .unwrap_or_default()
}

/// `value` rounded into the `u16` range.
fn saturating_u16(value: f32) -> u16 {
    num_traits::ToPrimitive::to_u16(&value.round().clamp(0.0, f32::from(u16::MAX)))
        .unwrap_or_default()
}

/// Why a host program could not be built.
#[derive(Debug, thiserror::Error)]
enum HostBuildError {
    /// The drawing program's header tables are unreadable.
    #[error("the drawing program's head or maxp table is unreadable")]
    Unreadable,
    /// A rebuilt table is invalid.
    #[error("{0}")]
    Table(String),
}

impl From<BuilderError> for HostBuildError {
    fn from(error: BuilderError) -> Self {
        Self::Table(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{HostFontProgram, HostGlyph};
    use crate::{
        font::{FontProgramFormat, FontSource, GlyphId},
        pdf_font_handle::LineMetrics,
    };
    use bytes::Bytes;
    use read_fonts::{
        FontRef, TableProvider,
        tables::gpos::{PositionLookup, SinglePos},
        types::GlyphId as ReadGlyphId,
    };

    #[test]
    fn host_program_lays_lines_and_comb_cells_out_as_the_canvas_does() {
        let source = FontSource::Memory {
            data: Bytes::from_static(include_bytes!("../assets/FoxitSans.cff")),
            format: FontProgramFormat::Cff,
            face_index: 0,
        };
        let line = LineMetrics {
            ascent: 0.75,
            descent: -0.25,
        };
        let program = HostFontProgram::new(&source, "pdf-field", &[], line, None)
            .unwrap()
            .unwrap();
        assert_eq!(program.line_height(), 1.0);
        let font = FontRef::new(program.data()).unwrap();
        let units = f32::from(font.head().unwrap().units_per_em());
        let hhea = font.hhea().unwrap();
        assert_eq!(f32::from(hhea.ascender().to_i16()), 0.75 * units);
        assert_eq!(f32::from(hhea.descender().to_i16()), -0.25 * units);
        assert_eq!(hhea.line_gap().to_i16(), 0);
        let os2 = font.os2().unwrap();
        assert_eq!(os2.s_typo_ascender(), hhea.ascender().to_i16());
        assert_eq!(os2.s_typo_descender(), hhea.descender().to_i16());

        // A comb program advances a 250-unit glyph by the 1000-unit cell and places it
        // in the middle of the cell.
        let glyph = HostGlyph {
            character: 'i',
            glyph: GlyphId(1),
            advance: 250.0,
        };
        let comb = HostFontProgram::new(&source, "pdf-comb", &[glyph], line, Some(1000.0))
            .unwrap()
            .unwrap();
        let font = FontRef::new(comb.data()).unwrap();
        let advance = font.hmtx().unwrap().advance(ReadGlyphId::new(1)).unwrap();
        assert_eq!(f32::from(advance), units);
        let lookup = font
            .gpos()
            .unwrap()
            .lookup_list()
            .unwrap()
            .lookups()
            .get(0)
            .unwrap();
        let single = match lookup {
            PositionLookup::Single(lookup) => match lookup.subtables().get(0) {
                Ok(SinglePos::Format2(single)) => Some(single),
                _ => None,
            },
            _ => None,
        }
        .unwrap();
        let placement = single
            .value_records()
            .get(0)
            .unwrap()
            .x_placement()
            .unwrap();
        assert_eq!(f32::from(placement), 375.0 * units / 1000.0);
    }
}
