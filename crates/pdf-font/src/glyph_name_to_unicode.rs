/// Map a PostScript / PDF glyph name to its Unicode scalar value.
///
/// Resolution order (mirrors the Adobe Glyph List specification):
///
/// 1. **Single-character names** — a one-character name whose sole character
///    is ASCII printable maps directly (e.g. `"A"` → `'A'`).
/// 2. **`uniXXXX` names** — four uppercase hex digits after `"uni"` give
///    the Unicode BMP codepoint (e.g. `"uni00E9"` → `'é'`).
/// 3. **`uXXXX` / `uXXXXXX` names** — four-to-six hex digits after `"u"`
///    give the Unicode codepoint (e.g. `"u1F600"` → `'😀'`).
/// 4. **Static AGL subset table** — a binary-searched sorted slice covering
///    the named entries that appear in StandardEncoding, WinAnsiEncoding,
///    MacRomanEncoding, and MacExpertEncoding.
///
/// Returns `None` for unknown names (e.g. `".notdef"`).
pub fn glyph_name_to_unicode(name: &str) -> Option<char> {
    // Rule 1: single printable ASCII character
    if name.len() == 1 {
        return name.chars().next().filter(|c| !c.is_control());
    }

    // Rule 2: "uniXXXX" — exactly four uppercase hex digits
    if let Some(rest) = name.strip_prefix("uni")
        && rest.len() == 4
        && rest.chars().all(|c| c.is_ascii_hexdigit())
        && let Ok(cp) = u32::from_str_radix(rest, 16)
    {
        return char::from_u32(cp);
    }

    // Rule 3: "uXXXX" or "uXXXXXX" — 4 to 6 hex digits
    if let Some(rest) = name.strip_prefix('u') {
        let len = rest.len();
        if (4..=6).contains(&len)
            && rest.chars().all(|c| c.is_ascii_hexdigit())
            && let Ok(cp) = u32::from_str_radix(rest, 16)
        {
            return char::from_u32(cp);
        }
    }

    // Rule 4: static AGL subset table
    AGL_TABLE
        .binary_search_by_key(&name, |&(n, _)| n)
        .ok()
        .and_then(|i| AGL_TABLE.get(i))
        .map(|&(_, c)| c)
}

/// Sorted subset of the Adobe Glyph List covering entries that appear in
/// StandardEncoding, WinAnsiEncoding, MacRomanEncoding, and MacExpertEncoding.
///
/// The table is sorted lexicographically (byte order) for `binary_search_by_key`.
/// Uppercase ASCII letters (65–90) sort before lowercase (97–122).
const AGL_TABLE: &[(&str, char)] = &[
    ("AE", '\u{00C6}'),
    ("Aacute", '\u{00C1}'),
    ("Acircumflex", '\u{00C2}'),
    ("Adieresis", '\u{00C4}'),
    ("Agrave", '\u{00C0}'),
    ("Aring", '\u{00C5}'),
    ("Atilde", '\u{00C3}'),
    ("Ccedilla", '\u{00C7}'),
    ("Eacute", '\u{00C9}'),
    ("Ecircumflex", '\u{00CA}'),
    ("Edieresis", '\u{00CB}'),
    ("Egrave", '\u{00C8}'),
    ("Eth", '\u{00D0}'),
    ("Euro", '\u{20AC}'),
    ("Iacute", '\u{00CD}'),
    ("Icircumflex", '\u{00CE}'),
    ("Idieresis", '\u{00CF}'),
    ("Igrave", '\u{00CC}'),
    ("Lslash", '\u{0141}'),
    ("Ntilde", '\u{00D1}'),
    ("OE", '\u{0152}'),
    ("Oacute", '\u{00D3}'),
    ("Ocircumflex", '\u{00D4}'),
    ("Odieresis", '\u{00D6}'),
    ("Ograve", '\u{00D2}'),
    ("Oslash", '\u{00D8}'),
    ("Otilde", '\u{00D5}'),
    ("Scaron", '\u{0160}'),
    ("Thorn", '\u{00DE}'),
    ("Uacute", '\u{00DA}'),
    ("Ucircumflex", '\u{00DB}'),
    ("Udieresis", '\u{00DC}'),
    ("Ugrave", '\u{00D9}'),
    ("Yacute", '\u{00DD}'),
    ("Ydieresis", '\u{0178}'),
    ("Zcaron", '\u{017D}'),
    ("aacute", '\u{00E1}'),
    ("acircumflex", '\u{00E2}'),
    ("acute", '\u{00B4}'),
    ("adieresis", '\u{00E4}'),
    ("ae", '\u{00E6}'),
    ("agrave", '\u{00E0}'),
    ("ampersand", '\u{0026}'),
    ("aring", '\u{00E5}'),
    ("asciicircum", '\u{005E}'),
    ("asciitilde", '\u{007E}'),
    ("asterisk", '\u{002A}'),
    ("at", '\u{0040}'),
    ("atilde", '\u{00E3}'),
    ("backslash", '\u{005C}'),
    ("bar", '\u{007C}'),
    ("braceleft", '\u{007B}'),
    ("braceright", '\u{007D}'),
    ("bracketleft", '\u{005B}'),
    ("bracketright", '\u{005D}'),
    ("breve", '\u{02D8}'),
    ("brokenbar", '\u{00A6}'),
    ("bullet", '\u{2022}'),
    ("caron", '\u{02C7}'),
    ("ccedilla", '\u{00E7}'),
    ("cedilla", '\u{00B8}'),
    ("cent", '\u{00A2}'),
    ("circumflex", '\u{02C6}'),
    ("colon", '\u{003A}'),
    ("colonmonetary", '\u{20A1}'),
    ("comma", '\u{002C}'),
    ("copyright", '\u{00A9}'),
    ("currency", '\u{00A4}'),
    ("dagger", '\u{2020}'),
    ("daggerdbl", '\u{2021}'),
    ("degree", '\u{00B0}'),
    ("dieresis", '\u{00A8}'),
    ("divide", '\u{00F7}'),
    ("dollar", '\u{0024}'),
    ("dotaccent", '\u{02D9}'),
    ("dotlessi", '\u{0131}'),
    ("eacute", '\u{00E9}'),
    ("ecircumflex", '\u{00EA}'),
    ("edieresis", '\u{00EB}'),
    ("egrave", '\u{00E8}'),
    ("eight", '\u{0038}'),
    ("ellipsis", '\u{2026}'),
    ("emdash", '\u{2014}'),
    ("endash", '\u{2013}'),
    ("equal", '\u{003D}'),
    ("eth", '\u{00F0}'),
    ("exclam", '\u{0021}'),
    ("exclamdown", '\u{00A1}'),
    ("ff", '\u{FB00}'),
    ("ffi", '\u{FB03}'),
    ("ffl", '\u{FB04}'),
    ("fi", '\u{FB01}'),
    ("five", '\u{0035}'),
    ("fiveeighths", '\u{215D}'),
    ("fl", '\u{FB02}'),
    ("florin", '\u{0192}'),
    ("four", '\u{0034}'),
    ("fraction", '\u{2044}'),
    ("germandbls", '\u{00DF}'),
    ("grave", '\u{0060}'),
    ("greater", '\u{003E}'),
    ("guillemotleft", '\u{00AB}'),
    ("guillemotright", '\u{00BB}'),
    ("guilsinglleft", '\u{2039}'),
    ("guilsinglright", '\u{203A}'),
    ("hungarumlaut", '\u{02DD}'),
    ("hyphen", '\u{002D}'),
    ("iacute", '\u{00ED}'),
    ("icircumflex", '\u{00EE}'),
    ("idieresis", '\u{00EF}'),
    ("igrave", '\u{00EC}'),
    ("less", '\u{003C}'),
    ("logicalnot", '\u{00AC}'),
    ("lslash", '\u{0142}'),
    ("macron", '\u{00AF}'),
    ("minus", '\u{2212}'),
    ("mu", '\u{00B5}'),
    ("multiply", '\u{00D7}'),
    ("nbspace", '\u{00A0}'),
    ("nine", '\u{0039}'),
    ("ntilde", '\u{00F1}'),
    ("numbersign", '\u{0023}'),
    ("oacute", '\u{00F3}'),
    ("ocircumflex", '\u{00F4}'),
    ("odieresis", '\u{00F6}'),
    ("oe", '\u{0153}'),
    ("ogonek", '\u{02DB}'),
    ("ograve", '\u{00F2}'),
    ("one", '\u{0031}'),
    ("oneeighth", '\u{215B}'),
    ("onehalf", '\u{00BD}'),
    ("onequarter", '\u{00BC}'),
    ("onesuperior", '\u{00B9}'),
    ("onethird", '\u{2153}'),
    ("ordfeminine", '\u{00AA}'),
    ("ordmasculine", '\u{00BA}'),
    ("oslash", '\u{00F8}'),
    ("otilde", '\u{00F5}'),
    ("paragraph", '\u{00B6}'),
    ("parenleft", '\u{0028}'),
    ("parenright", '\u{0029}'),
    ("percent", '\u{0025}'),
    ("period", '\u{002E}'),
    ("periodcentered", '\u{00B7}'),
    ("perthousand", '\u{2030}'),
    ("plus", '\u{002B}'),
    ("plusminus", '\u{00B1}'),
    ("question", '\u{003F}'),
    ("questiondown", '\u{00BF}'),
    ("quotedbl", '\u{0022}'),
    ("quotedblbase", '\u{201E}'),
    ("quotedblleft", '\u{201C}'),
    ("quotedblright", '\u{201D}'),
    ("quoteleft", '\u{2018}'),
    ("quoteright", '\u{2019}'),
    ("quotesinglbase", '\u{201A}'),
    ("quotesingle", '\u{0027}'),
    ("registered", '\u{00AE}'),
    ("ring", '\u{02DA}'),
    ("rupiah", '\u{20A8}'),
    ("scaron", '\u{0161}'),
    ("section", '\u{00A7}'),
    ("semicolon", '\u{003B}'),
    ("seven", '\u{0037}'),
    ("seveneighths", '\u{215E}'),
    ("six", '\u{0036}'),
    ("slash", '\u{002F}'),
    ("softhyphen", '\u{00AD}'),
    ("space", '\u{0020}'),
    ("sterling", '\u{00A3}'),
    ("thorn", '\u{00FE}'),
    ("three", '\u{0033}'),
    ("threeeighths", '\u{215C}'),
    ("threequarters", '\u{00BE}'),
    ("threesuperior", '\u{00B3}'),
    ("tilde", '\u{02DC}'),
    ("trademark", '\u{2122}'),
    ("two", '\u{0032}'),
    ("twosuperior", '\u{00B2}'),
    ("twothirds", '\u{2154}'),
    ("uacute", '\u{00FA}'),
    ("ucircumflex", '\u{00FB}'),
    ("udieresis", '\u{00FC}'),
    ("ugrave", '\u{00F9}'),
    ("underscore", '\u{005F}'),
    ("yacute", '\u{00FD}'),
    ("ydieresis", '\u{00FF}'),
    ("yen", '\u{00A5}'),
    ("zcaron", '\u{017E}'),
    ("zero", '\u{0030}'),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_char() {
        assert_eq!(glyph_name_to_unicode("A"), Some('A'));
        assert_eq!(glyph_name_to_unicode("z"), Some('z'));
    }

    #[test]
    fn test_uni_prefix() {
        assert_eq!(glyph_name_to_unicode("uni00E9"), Some('\u{00E9}'));
        assert_eq!(glyph_name_to_unicode("uni0041"), Some('A'));
    }

    #[test]
    fn test_u_prefix() {
        assert_eq!(glyph_name_to_unicode("u00E9"), Some('\u{00E9}'));
        assert_eq!(glyph_name_to_unicode("u1F600"), char::from_u32(0x1F600));
    }

    #[test]
    fn test_agl_table() {
        assert_eq!(glyph_name_to_unicode("germandbls"), Some('\u{00DF}'));
        assert_eq!(glyph_name_to_unicode("eacute"), Some('\u{00E9}'));
        assert_eq!(glyph_name_to_unicode("OE"), Some('\u{0152}'));
        assert_eq!(glyph_name_to_unicode("endash"), Some('\u{2013}'));
        assert_eq!(glyph_name_to_unicode("bullet"), Some('\u{2022}'));
    }

    #[test]
    fn test_notdef() {
        assert_eq!(glyph_name_to_unicode(".notdef"), None);
    }

    #[test]
    fn test_table_is_sorted() {
        let names: Vec<_> = AGL_TABLE.iter().map(|&(n, _)| n).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "AGL_TABLE must be sorted by name");
    }

    #[test]
    fn test_agl_new_entries() {
        assert_eq!(glyph_name_to_unicode("ff"), Some('\u{FB00}'));
        assert_eq!(glyph_name_to_unicode("ffi"), Some('\u{FB03}'));
        assert_eq!(glyph_name_to_unicode("ffl"), Some('\u{FB04}'));
        assert_eq!(glyph_name_to_unicode("oneeighth"), Some('\u{215B}'));
        assert_eq!(glyph_name_to_unicode("nbspace"), Some('\u{00A0}'));
        assert_eq!(glyph_name_to_unicode("softhyphen"), Some('\u{00AD}'));
        assert_eq!(glyph_name_to_unicode("threeeighths"), Some('\u{215C}'));
        assert_eq!(glyph_name_to_unicode("fiveeighths"), Some('\u{215D}'));
        assert_eq!(glyph_name_to_unicode("seveneighths"), Some('\u{215E}'));
        assert_eq!(glyph_name_to_unicode("onethird"), Some('\u{2153}'));
        assert_eq!(glyph_name_to_unicode("twothirds"), Some('\u{2154}'));
        assert_eq!(glyph_name_to_unicode("colonmonetary"), Some('\u{20A1}'));
        assert_eq!(glyph_name_to_unicode("rupiah"), Some('\u{20A8}'));
    }
}
