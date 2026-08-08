use std::collections::HashMap;

use pdf_cmap::{error::CMapError, predefined::PredefinedCMap};
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::error::FontError;

/// Known Adobe CIDSystemInfo ordering values with bundled CJK font support.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CidOrdering {
    /// Adobe-Japan1 character collection.
    Japan1,
    /// Adobe-GB1 character collection.
    GB1,
    /// Adobe-CNS1 character collection.
    CNS1,
    /// Adobe-Korea1 character collection.
    Korea1,
}

impl CidOrdering {
    /// Extract a supported CID ordering from a font dictionary's `/CIDSystemInfo`.
    ///
    /// # Paramaters
    ///
    /// - `dictionary`: The PDF font dictionary that may contain `/CIDSystemInfo`.
    /// - `objects`: The resolver used to dereference indirect PDF objects.
    ///
    /// # Returns
    ///
    /// A known [`CidOrdering`] when `/CIDSystemInfo /Ordering` is present and
    /// supported, or `None` when the entry is absent or unknown.
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, FontError> {
        let Some(cid_system_info) = dictionary.optional_dictionary("CIDSystemInfo", objects)?
        else {
            return Ok(None);
        };

        let Some(ordering) = cid_system_info.optional_str("Ordering", objects)? else {
            return Ok(None);
        };

        Ok(Self::from_name(ordering))
    }

    /// Build a best-effort CID to Unicode map for this ordering.
    pub(crate) fn cid_to_unicode_map(self) -> Result<Option<HashMap<u16, char>>, CMapError> {
        let unicode_cmap_name = match self {
            Self::Japan1 => "UniJIS-UCS2-HW-H",
            Self::GB1 => "UniGB-UCS2-H",
            Self::CNS1 => "UniCNS-UCS2-H",
            Self::Korea1 => "UniKS-UCS2-H",
        };

        Ok(PredefinedCMap::from_name(unicode_cmap_name)?.map(|cmap| cmap.cid_to_unicode_map()))
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "Japan1" => Some(Self::Japan1),
            "GB1" => Some(Self::GB1),
            "CNS1" => Some(Self::CNS1),
            "Korea1" => Some(Self::Korea1),
            _ => None,
        }
    }
}
