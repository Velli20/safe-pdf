mod generated;

use std::collections::{BTreeSet, HashMap};

use crate::{WritingMode, cmap_support::Type0CodeMap, error::CMapError};

/// Generated predefined CMap metadata.
#[derive(Debug, Clone, Copy)]
pub struct GeneratedCMap {
    /// CMap resource name.
    pub name: &'static str,
    /// Optional base CMap name used through `usecmap`.
    pub use_cmap: Option<&'static str>,
    /// `0` for horizontal writing mode, `1` for vertical writing mode.
    pub writing_mode: u8,
    /// Valid input byte ranges.
    pub code_space_ranges: &'static [GeneratedCodeSpaceRange],
    /// Sequential code-to-CID ranges sorted by source start.
    pub cid_ranges: &'static [GeneratedCidRange],
    /// Explicit code-to-CID mappings sorted by source code.
    pub cid_chars: &'static [GeneratedCidChar],
}

/// Generated codespace range.
#[derive(Debug, Clone, Copy)]
pub struct GeneratedCodeSpaceRange {
    /// Packed big-endian inclusive start code.
    pub start: u32,
    /// Packed big-endian inclusive end code.
    pub end: u32,
    /// Source code byte length.
    pub len: u8,
}

/// Generated sequential code-to-CID range.
#[derive(Debug, Clone, Copy)]
pub struct GeneratedCidRange {
    /// Packed big-endian inclusive source start code.
    pub start: u32,
    /// Packed big-endian inclusive source end code.
    pub end: u32,
    /// First CID in the sequential destination range.
    pub cid_start: u16,
}

/// Generated explicit code-to-CID mapping.
#[derive(Debug, Clone, Copy)]
pub struct GeneratedCidChar {
    /// Packed big-endian source code.
    pub code: u32,
    /// Destination CID.
    pub cid: u16,
}

/// A resolved predefined CMap and its `UseCMap` fallback chain.
#[derive(Debug, Clone)]
pub struct PredefinedCMap {
    maps: Vec<&'static GeneratedCMap>,
}

/// Known Adobe CIDSystemInfo ordering values with bundled Unicode CMap support.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CidOrdering {
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
    /// Resolve a known Adobe CIDSystemInfo ordering value.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "Japan1" => Some(Self::Japan1),
            "GB1" => Some(Self::GB1),
            "CNS1" => Some(Self::CNS1),
            "Korea1" => Some(Self::Korea1),
            _ => None,
        }
    }

    /// Return whether an ordering name has bundled CJK fallback support.
    pub fn is_known_cjk_name(name: &str) -> bool {
        Self::from_name(name).is_some()
    }

    /// Build a best-effort CID to Unicode map for this ordering.
    pub fn cid_to_unicode_map(self) -> Result<Option<HashMap<u16, char>>, CMapError> {
        Ok(PredefinedCMap::from_name(self.unicode_cmap_name())?
            .map(|cmap| cmap.cid_to_unicode_map()))
    }

    fn unicode_cmap_name(self) -> &'static str {
        match self {
            Self::Japan1 => "UniJIS-UCS2-HW-H",
            Self::GB1 => "UniGB-UCS2-H",
            Self::CNS1 => "UniCNS-UCS2-H",
            Self::Korea1 => "UniKS-UCS2-H",
        }
    }
}

impl PredefinedCMap {
    /// Resolve a predefined CMap by name.
    pub fn from_name(name: &str) -> Result<Option<Self>, CMapError> {
        let Some(root) = find_cmap(name) else {
            return Ok(None);
        };

        let mut maps = vec![root];
        let mut seen = BTreeSet::new();
        seen.insert(root.name);

        let mut current = root;
        while let Some(use_cmap) = current.use_cmap {
            if !seen.insert(use_cmap) {
                return Err(CMapError::InvalidType0EncodingCMap(format!(
                    "recursive UseCMap reference for {use_cmap}"
                )));
            }
            let Some(next) = find_cmap(use_cmap) else {
                return Err(CMapError::InvalidType0EncodingCMap(format!(
                    "missing UseCMap dependency {use_cmap}"
                )));
            };
            maps.push(next);
            current = next;
        }

        Ok(Some(Self { maps }))
    }

    /// Return the writing mode declared by the root CMap.
    pub fn writing_mode(&self) -> WritingMode {
        self.maps
            .first()
            .map(|map| {
                if map.writing_mode == 1 {
                    WritingMode::Vertical
                } else {
                    WritingMode::Horizontal
                }
            })
            .unwrap_or(WritingMode::Horizontal)
    }

    /// Build a best-effort CID to Unicode map by inverting this Unicode CMap.
    pub fn cid_to_unicode_map(&self) -> HashMap<u16, char> {
        let mut result = HashMap::new();
        for map in self.maps.iter().rev() {
            for range in map.cid_ranges {
                insert_cid_range_unicode(&mut result, range);
            }
            for ch in map.cid_chars {
                if let Some(c) = char::from_u32(ch.code) {
                    result.insert(ch.cid, c);
                }
            }
        }
        result
    }
}

impl Type0CodeMap for PredefinedCMap {
    /// Return all code lengths accepted by this CMap chain.
    fn allowed_code_lengths(&self) -> Vec<usize> {
        let mut lengths = BTreeSet::new();
        for map in &self.maps {
            for range in map.code_space_ranges {
                lengths.insert(usize::from(range.len));
            }
        }
        lengths.into_iter().collect()
    }

    /// Return whether a packed code is valid for a byte length in this CMap chain.
    fn has_code_space(&self, code: u32, len: usize) -> bool {
        self.maps.iter().any(|map| {
            map.code_space_ranges.iter().any(|range| {
                usize::from(range.len) == len && range.start <= code && code <= range.end
            })
        })
    }

    /// Resolve one packed source code to a CID through root and fallback maps.
    fn map_code_to_cid(&self, code: u32) -> Option<u16> {
        for map in &self.maps {
            if let Some(cid) = find_cid_char(map.cid_chars, code) {
                return Some(cid);
            }
            if let Some(cid) = find_cid_range(map.cid_ranges, code) {
                return Some(cid);
            }
        }
        None
    }
}

/// Build a best-effort CID to Unicode map for a known Adobe CIDSystemInfo ordering.
pub fn cid_to_unicode_map_for_ordering(
    ordering: &str,
) -> Result<Option<HashMap<u16, char>>, CMapError> {
    let Some(ordering) = CidOrdering::from_name(ordering) else {
        return Ok(None);
    };

    ordering.cid_to_unicode_map()
}

/// Find a generated CMap by resource name.
fn find_cmap(name: &str) -> Option<&'static GeneratedCMap> {
    generated::CMAPS
        .binary_search_by(|cmap| cmap.name.cmp(name))
        .ok()
        .and_then(|index| generated::CMAPS.get(index))
}

/// Find an explicit generated code-to-CID mapping.
fn find_cid_char(chars: &[GeneratedCidChar], code: u32) -> Option<u16> {
    chars
        .binary_search_by_key(&code, |entry| entry.code)
        .ok()
        .and_then(|index| chars.get(index).map(|entry| entry.cid))
}

/// Find a generated sequential code-to-CID range containing a source code.
fn find_cid_range(ranges: &[GeneratedCidRange], code: u32) -> Option<u16> {
    let mut left = 0usize;
    let mut right = ranges.len();

    while left < right {
        let mid = left.checked_add((right.checked_sub(left)?) / 2)?;
        let range = ranges.get(mid)?;
        if code < range.start {
            right = mid;
        } else if code > range.end {
            left = mid.saturating_add(1);
        } else {
            let offset = code.checked_sub(range.start)?;
            let offset = u16::try_from(offset).ok()?;
            return range.cid_start.checked_add(offset);
        }
    }

    None
}

fn insert_cid_range_unicode(result: &mut HashMap<u16, char>, range: &GeneratedCidRange) {
    let mut code = range.start;
    while code <= range.end {
        let Some(offset) = code.checked_sub(range.start) else {
            break;
        };
        let Ok(offset) = u16::try_from(offset) else {
            break;
        };
        let Some(cid) = range.cid_start.checked_add(offset) else {
            break;
        };
        if let Some(c) = char::from_u32(code) {
            result.insert(cid, c);
        }
        let Some(next) = code.checked_add(1) else {
            break;
        };
        code = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn japan1_cid_to_unicode_includes_half_width_ascii_and_space_variants() {
        let map = cid_to_unicode_map_for_ordering("Japan1").unwrap().unwrap();

        assert_eq!(map.get(&231), Some(&' '));
        assert_eq!(map.get(&633), Some(&'\u{2003}'));
    }

    #[test]
    fn cid_ordering_rejects_unknown_orderings() {
        assert!(CidOrdering::from_name("Unknown").is_none());
        assert!(!CidOrdering::is_known_cjk_name("Unknown"));
        assert!(
            cid_to_unicode_map_for_ordering("Unknown")
                .unwrap()
                .is_none()
        );
    }
}
