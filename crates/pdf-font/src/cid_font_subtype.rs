//! CIDFont subtype parsing.

use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::error::FontError;

/// CIDFont subtypes supported by the parser.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CidFontSubType {
    /// Type 1/CFF based CID-keyed font.
    Type0,
    /// TrueType based CID-keyed font.
    Type2,
}

impl CidFontSubType {
    /// Parse the CIDFont subtype from a descendant font dictionary.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, FontError> {
        match dictionary.required_str("Subtype", objects)? {
            "CIDFontType0" => Ok(Self::Type0),
            "CIDFontType2" => Ok(Self::Type2),
            other => Err(FontError::UnsupportedCidFontSubtype {
                subtype: other.to_string(),
            }),
        }
    }
}
