//! PDF Type 0 CID-keyed CFF support backed by `skrifa`.
//!
//! A Type 0 font with a `CIDFontType0` descendant addresses glyphs by CID,
//! while CFF charstrings are indexed by physical glyph identifier. The CFF
//! charset is therefore the mapping boundary for every face query. Type 0
//! fonts with `CIDFontType2` descendants use the crate's TrueType driver after
//! the PDF layer applies any `CIDToGIDMap`.
//!
//! Two physical representations are supported behind one driver:
//!
//! - `raw` handles standalone CFF data from a PDF `CIDFontType0C` stream.
//! - `open_type` handles a CFF table inside an OpenType font or collection.
//!
//! Both concrete faces expose [`GlyphId`] values as CIDs rather than physical
//! CFF GIDs. The driver selects the concrete implementation once during load,
//! leaving metrics and outline queries free of representation branches.

#[path = "type0_open_type.rs"]
mod open_type;
#[path = "type0_raw.rs"]
mod raw;

use std::sync::Arc;

use bytes::Bytes;
use read_fonts::ps::{
    cff::{CffFontRef, charset::Charset},
    string::Sid,
};

use crate::error::FontError;
use crate::font::{
    FontDriver, FontFace, FontFaceId, FontLoadRequest, FontMetadata, FontProgramFormat, FontSource,
    GlyphId,
};
use crate::query_cache::QueryCache;

use self::open_type::OpenTypeType0FontFace;
use self::raw::RawType0FontFace;

/// A `skrifa`-backed loader for CID-keyed CFF descendants of PDF Type 0 fonts.
///
/// The driver accepts raw CID-CFF data and OpenType containers with CFF1
/// outlines. Parsing and structural validation happen in [`FontDriver::load`]
/// so malformed programs fail before a face is returned. The concrete face
/// types remain private because the driver exposes them through the common
/// [`FontFace`] contract.
pub struct Type0FontDriver;

impl Type0FontDriver {
    /// Creates a Type 0 CID-CFF driver.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for Type0FontDriver {
    /// Creates a Type 0 CID-CFF driver.
    fn default() -> Self {
        Self::new()
    }
}

impl FontDriver for Type0FontDriver {
    /// Reports whether `format` can represent a CID-keyed CFF descendant.
    ///
    /// `OpenTypeCff` is accepted provisionally. Loading still verifies that
    /// its CFF table is CID-keyed, because the same container format can hold
    /// a name-keyed simple font.
    fn supports(&self, format: FontProgramFormat) -> bool {
        matches!(
            format,
            FontProgramFormat::CidCff | FontProgramFormat::OpenTypeCff
        )
    }

    /// Validates and loads one in-memory CID-CFF face.
    ///
    /// External sources must first be resolved by the application into memory.
    /// A raw CFF `face_index` selects a top dictionary, while an OpenType index
    /// selects a member of a font collection.
    fn load(&self, request: &FontLoadRequest) -> Result<Arc<dyn FontFace>, FontError> {
        let FontSource::Memory {
            data,
            format,
            face_index,
        } = &request.source
        else {
            return Err(FontError::InvalidProgram {
                format: Option::<FontProgramFormat>::from(&request.source)
                    .unwrap_or(FontProgramFormat::CidCff),
                message: "the Type 0 driver requires an in-memory font program".into(),
            });
        };

        if !self.supports(*format) {
            return Err(FontError::DriverUnavailable { format: *format });
        }

        let common = Type0FaceData {
            id: request.face_id,
            data: data.clone(),
            face_index: *face_index,
            metadata: request.metadata_hint.clone(),
        };

        match format {
            FontProgramFormat::CidCff => Ok(Arc::new(RawType0FontFace::new(common)?)),
            FontProgramFormat::OpenTypeCff => Ok(Arc::new(OpenTypeType0FontFace::new(common)?)),
            _ => Err(FontError::DriverUnavailable { format: *format }),
        }
    }
}

/// State shared by both physical Type 0 face implementations.
///
/// Each concrete face moves this owned state into a cell containing the parser
/// views that borrow `data`.
pub(crate) struct Type0FaceData {
    /// Identity allocated by [`Type0FontDriver`].
    pub(super) id: FontFaceId,
    /// Shared immutable bytes containing the physical font program.
    pub(super) data: Bytes,
    /// Raw CFF top-dictionary index or OpenType collection-member index.
    pub(super) face_index: u32,
    /// PDF- or application-provided metadata used for font matching.
    pub(super) metadata: FontMetadata,
}

/// Validates the CFF invariants required by both representations.
///
/// A parseable CFF program is not necessarily suitable for a PDF Type 0
/// descendant. Name-keyed CFF uses charset values as string identifiers, while
/// Type 0 rendering requires a CID-keyed program with a readable CID charset.
pub(crate) fn validate_cid_cff(
    cff: &CffFontRef<'_>,
    format: FontProgramFormat,
) -> Result<(), FontError> {
    if !cff.is_cid() {
        return Err(FontError::InvalidProgram {
            format,
            message: "the CFF program is not CID-keyed".into(),
        });
    }
    if cff.charset().is_none() {
        return Err(FontError::InvalidProgram {
            format,
            message: "the CID-keyed CFF program does not contain a charset".into(),
        });
    }
    Ok(())
}

/// Lazily cached bidirectional mapping between CIDs and physical CFF glyph IDs.
pub(crate) struct CidGlyphMap<'a> {
    /// Borrowed CFF charset containing the authoritative mappings.
    charset: Charset<'a>,
    /// Physical glyph results, including misses, indexed by public CID.
    glyphs_by_cid: QueryCache<u16, Option<read_fonts::types::GlyphId>>,
    /// Public CID results, including misses, indexed by physical glyph ID.
    cids_by_glyph: QueryCache<read_fonts::types::GlyphId, Option<u16>>,
}

impl<'a> CidGlyphMap<'a> {
    /// Retains a validated charset and initializes empty per-key caches.
    pub(crate) fn new(cff: &CffFontRef<'a>, format: FontProgramFormat) -> Result<Self, FontError> {
        let charset = cff.charset().ok_or_else(|| FontError::InvalidProgram {
            format,
            message: "the CID-keyed CFF program does not contain a charset".into(),
        })?;
        Ok(Self {
            charset,
            glyphs_by_cid: QueryCache::new(),
            cids_by_glyph: QueryCache::new(),
        })
    }

    /// Resolves a public CID to its physical CFF glyph identifier.
    pub(crate) fn glyph_id(
        &self,
        face_id: FontFaceId,
        glyph: GlyphId,
    ) -> Result<read_fonts::types::GlyphId, FontError> {
        let cid = u16::try_from(glyph.0).map_err(|_| FontError::MissingGlyph {
            face_id,
            glyph_id: glyph,
        })?;
        if let Some(resolved) = self.glyphs_by_cid.get(&cid) {
            return resolved.ok_or(FontError::MissingGlyph {
                face_id,
                glyph_id: glyph,
            });
        }

        // `Charset::glyph_id` may scan a custom CFF charset. Cache both hits
        // and misses so each CID pays that cost at most once per loaded face.
        let resolved = self.charset.glyph_id(Sid::new(cid)).ok();
        self.glyphs_by_cid.insert(cid, resolved);
        resolved.ok_or(FontError::MissingGlyph {
            face_id,
            glyph_id: glyph,
        })
    }

    /// Resolves a physical CFF glyph identifier to its public CID.
    pub(crate) fn cid(&self, glyph: read_fonts::types::GlyphId) -> Option<GlyphId> {
        if let Some(cid) = self.cids_by_glyph.get(&glyph) {
            return cid.map(u32::from).map(GlyphId);
        }

        // Reverse lookup is used after an OpenType cmap resolves a Unicode
        // scalar to a physical glyph. It is cached independently from the
        // forward CID path because either direction may be used first.
        let cid = self.charset.string_id(glyph).ok().map(|cid| cid.to_u16());
        self.cids_by_glyph.insert(glyph, cid);
        cid.map(u32::from).map(GlyphId)
    }
}

/// Converts a public CID into the physical GID used to index CFF charstrings.
///
/// Values outside the 16-bit CFF range and CIDs absent from the retained
/// charset are clean missing-glyph results rather than malformed-program
/// errors.
pub(crate) fn cff_glyph_id(
    glyphs: &CidGlyphMap<'_>,
    face_id: FontFaceId,
    glyph: GlyphId,
) -> Result<read_fonts::types::GlyphId, FontError> {
    glyphs.glyph_id(face_id, glyph)
}
