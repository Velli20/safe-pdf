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
pub fn glyph_name_to_unicode(name: &[u8]) -> Option<char> {
    // Rule 1: single printable ASCII character
    if name.len() == 1 {
        return name
            .first()
            .copied()
            .map(char::from)
            .filter(|c| !c.is_control());
    }

    // Rule 2: "uniXXXX" — exactly four uppercase hex digits
    if let Some(rest) = name.strip_prefix(b"uni")
        && rest.len() == 4
        && let Some(cp) = parse_hex(rest)
    {
        return char::from_u32(cp);
    }

    // Rule 3: "uXXXX" or "uXXXXXX" — 4 to 6 hex digits
    if let Some(rest) = name.strip_prefix(b"u") {
        let len = rest.len();
        if (4..=6).contains(&len)
            && let Some(cp) = parse_hex(rest)
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

fn parse_hex(bytes: &[u8]) -> Option<u32> {
    bytes.iter().try_fold(0_u32, |value, byte| {
        let digit = match byte {
            b'0'..=b'9' => u32::from(byte.saturating_sub(b'0')),
            b'A'..=b'F' => u32::from(byte.saturating_sub(b'A')).checked_add(10)?,
            b'a'..=b'f' => u32::from(byte.saturating_sub(b'a')).checked_add(10)?,
            _ => return None,
        };
        value.checked_mul(16)?.checked_add(digit)
    })
}

/// Sorted subset of the Adobe Glyph List covering entries that appear in
/// StandardEncoding, WinAnsiEncoding, MacRomanEncoding, and MacExpertEncoding.
///
/// The table is sorted lexicographically (byte order) for `binary_search_by_key`.
/// Uppercase ASCII letters (65–90) sort before lowercase (97–122).
const AGL_TABLE: &[(&[u8], char)] = &[
    (b"AE", '\u{00C6}'),
    (b"Aacute", '\u{00C1}'),
    (b"Acircumflex", '\u{00C2}'),
    (b"Adieresis", '\u{00C4}'),
    (b"Agrave", '\u{00C0}'),
    (b"Aring", '\u{00C5}'),
    (b"Atilde", '\u{00C3}'),
    (b"Ccedilla", '\u{00C7}'),
    (b"Eacute", '\u{00C9}'),
    (b"Ecircumflex", '\u{00CA}'),
    (b"Edieresis", '\u{00CB}'),
    (b"Egrave", '\u{00C8}'),
    (b"Eth", '\u{00D0}'),
    (b"Euro", '\u{20AC}'),
    (b"Iacute", '\u{00CD}'),
    (b"Icircumflex", '\u{00CE}'),
    (b"Idieresis", '\u{00CF}'),
    (b"Igrave", '\u{00CC}'),
    (b"Lslash", '\u{0141}'),
    (b"Ntilde", '\u{00D1}'),
    (b"OE", '\u{0152}'),
    (b"Oacute", '\u{00D3}'),
    (b"Ocircumflex", '\u{00D4}'),
    (b"Odieresis", '\u{00D6}'),
    (b"Ograve", '\u{00D2}'),
    (b"Oslash", '\u{00D8}'),
    (b"Otilde", '\u{00D5}'),
    (b"Scaron", '\u{0160}'),
    (b"Thorn", '\u{00DE}'),
    (b"Uacute", '\u{00DA}'),
    (b"Ucircumflex", '\u{00DB}'),
    (b"Udieresis", '\u{00DC}'),
    (b"Ugrave", '\u{00D9}'),
    (b"Yacute", '\u{00DD}'),
    (b"Ydieresis", '\u{0178}'),
    (b"Zcaron", '\u{017D}'),
    (b"aacute", '\u{00E1}'),
    (b"acircumflex", '\u{00E2}'),
    (b"acute", '\u{00B4}'),
    (b"adieresis", '\u{00E4}'),
    (b"ae", '\u{00E6}'),
    (b"agrave", '\u{00E0}'),
    (b"ampersand", '\u{0026}'),
    (b"aring", '\u{00E5}'),
    (b"asciicircum", '\u{005E}'),
    (b"asciitilde", '\u{007E}'),
    (b"asterisk", '\u{002A}'),
    (b"at", '\u{0040}'),
    (b"atilde", '\u{00E3}'),
    (b"backslash", '\u{005C}'),
    (b"bar", '\u{007C}'),
    (b"braceleft", '\u{007B}'),
    (b"braceright", '\u{007D}'),
    (b"bracketleft", '\u{005B}'),
    (b"bracketright", '\u{005D}'),
    (b"breve", '\u{02D8}'),
    (b"brokenbar", '\u{00A6}'),
    (b"bullet", '\u{2022}'),
    (b"caron", '\u{02C7}'),
    (b"ccedilla", '\u{00E7}'),
    (b"cedilla", '\u{00B8}'),
    (b"cent", '\u{00A2}'),
    (b"circumflex", '\u{02C6}'),
    (b"colon", '\u{003A}'),
    (b"colonmonetary", '\u{20A1}'),
    (b"comma", '\u{002C}'),
    (b"copyright", '\u{00A9}'),
    (b"currency", '\u{00A4}'),
    (b"dagger", '\u{2020}'),
    (b"daggerdbl", '\u{2021}'),
    (b"degree", '\u{00B0}'),
    (b"dieresis", '\u{00A8}'),
    (b"divide", '\u{00F7}'),
    (b"dollar", '\u{0024}'),
    (b"dotaccent", '\u{02D9}'),
    (b"dotlessi", '\u{0131}'),
    (b"eacute", '\u{00E9}'),
    (b"ecircumflex", '\u{00EA}'),
    (b"edieresis", '\u{00EB}'),
    (b"egrave", '\u{00E8}'),
    (b"eight", '\u{0038}'),
    (b"ellipsis", '\u{2026}'),
    (b"emdash", '\u{2014}'),
    (b"endash", '\u{2013}'),
    (b"equal", '\u{003D}'),
    (b"eth", '\u{00F0}'),
    (b"exclam", '\u{0021}'),
    (b"exclamdown", '\u{00A1}'),
    (b"ff", '\u{FB00}'),
    (b"ffi", '\u{FB03}'),
    (b"ffl", '\u{FB04}'),
    (b"fi", '\u{FB01}'),
    (b"five", '\u{0035}'),
    (b"fiveeighths", '\u{215D}'),
    (b"fl", '\u{FB02}'),
    (b"florin", '\u{0192}'),
    (b"four", '\u{0034}'),
    (b"fraction", '\u{2044}'),
    (b"germandbls", '\u{00DF}'),
    (b"grave", '\u{0060}'),
    (b"greater", '\u{003E}'),
    (b"guillemotleft", '\u{00AB}'),
    (b"guillemotright", '\u{00BB}'),
    (b"guilsinglleft", '\u{2039}'),
    (b"guilsinglright", '\u{203A}'),
    (b"hungarumlaut", '\u{02DD}'),
    (b"hyphen", '\u{002D}'),
    (b"iacute", '\u{00ED}'),
    (b"icircumflex", '\u{00EE}'),
    (b"idieresis", '\u{00EF}'),
    (b"igrave", '\u{00EC}'),
    (b"less", '\u{003C}'),
    (b"logicalnot", '\u{00AC}'),
    (b"lslash", '\u{0142}'),
    (b"macron", '\u{00AF}'),
    (b"minus", '\u{2212}'),
    (b"mu", '\u{00B5}'),
    (b"multiply", '\u{00D7}'),
    (b"nbspace", '\u{00A0}'),
    (b"nine", '\u{0039}'),
    (b"ntilde", '\u{00F1}'),
    (b"numbersign", '\u{0023}'),
    (b"oacute", '\u{00F3}'),
    (b"ocircumflex", '\u{00F4}'),
    (b"odieresis", '\u{00F6}'),
    (b"oe", '\u{0153}'),
    (b"ogonek", '\u{02DB}'),
    (b"ograve", '\u{00F2}'),
    (b"one", '\u{0031}'),
    (b"oneeighth", '\u{215B}'),
    (b"onehalf", '\u{00BD}'),
    (b"onequarter", '\u{00BC}'),
    (b"onesuperior", '\u{00B9}'),
    (b"onethird", '\u{2153}'),
    (b"ordfeminine", '\u{00AA}'),
    (b"ordmasculine", '\u{00BA}'),
    (b"oslash", '\u{00F8}'),
    (b"otilde", '\u{00F5}'),
    (b"paragraph", '\u{00B6}'),
    (b"parenleft", '\u{0028}'),
    (b"parenright", '\u{0029}'),
    (b"percent", '\u{0025}'),
    (b"period", '\u{002E}'),
    (b"periodcentered", '\u{00B7}'),
    (b"perthousand", '\u{2030}'),
    (b"plus", '\u{002B}'),
    (b"plusminus", '\u{00B1}'),
    (b"question", '\u{003F}'),
    (b"questiondown", '\u{00BF}'),
    (b"quotedbl", '\u{0022}'),
    (b"quotedblbase", '\u{201E}'),
    (b"quotedblleft", '\u{201C}'),
    (b"quotedblright", '\u{201D}'),
    (b"quoteleft", '\u{2018}'),
    (b"quoteright", '\u{2019}'),
    (b"quotesinglbase", '\u{201A}'),
    (b"quotesingle", '\u{0027}'),
    (b"registered", '\u{00AE}'),
    (b"ring", '\u{02DA}'),
    (b"rupiah", '\u{20A8}'),
    (b"scaron", '\u{0161}'),
    (b"section", '\u{00A7}'),
    (b"semicolon", '\u{003B}'),
    (b"seven", '\u{0037}'),
    (b"seveneighths", '\u{215E}'),
    (b"six", '\u{0036}'),
    (b"slash", '\u{002F}'),
    (b"softhyphen", '\u{00AD}'),
    (b"space", '\u{0020}'),
    (b"sterling", '\u{00A3}'),
    (b"thorn", '\u{00FE}'),
    (b"three", '\u{0033}'),
    (b"threeeighths", '\u{215C}'),
    (b"threequarters", '\u{00BE}'),
    (b"threesuperior", '\u{00B3}'),
    (b"tilde", '\u{02DC}'),
    (b"trademark", '\u{2122}'),
    (b"two", '\u{0032}'),
    (b"twosuperior", '\u{00B2}'),
    (b"twothirds", '\u{2154}'),
    (b"uacute", '\u{00FA}'),
    (b"ucircumflex", '\u{00FB}'),
    (b"udieresis", '\u{00FC}'),
    (b"ugrave", '\u{00F9}'),
    (b"underscore", '\u{005F}'),
    (b"yacute", '\u{00FD}'),
    (b"ydieresis", '\u{00FF}'),
    (b"yen", '\u{00A5}'),
    (b"zcaron", '\u{017E}'),
    (b"zero", '\u{0030}'),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_char() {
        assert_eq!(glyph_name_to_unicode(b"A"), Some('A'));
        assert_eq!(glyph_name_to_unicode(b"z"), Some('z'));
    }

    #[test]
    fn test_uni_prefix() {
        assert_eq!(glyph_name_to_unicode(b"uni00E9"), Some('\u{00E9}'));
        assert_eq!(glyph_name_to_unicode(b"uni0041"), Some('A'));
    }

    #[test]
    fn test_u_prefix() {
        assert_eq!(glyph_name_to_unicode(b"u00E9"), Some('\u{00E9}'));
        assert_eq!(glyph_name_to_unicode(b"u1F600"), char::from_u32(0x1F600));
    }

    #[test]
    fn test_agl_table() {
        assert_eq!(glyph_name_to_unicode(b"germandbls"), Some('\u{00DF}'));
        assert_eq!(glyph_name_to_unicode(b"eacute"), Some('\u{00E9}'));
        assert_eq!(glyph_name_to_unicode(b"OE"), Some('\u{0152}'));
        assert_eq!(glyph_name_to_unicode(b"endash"), Some('\u{2013}'));
        assert_eq!(glyph_name_to_unicode(b"bullet"), Some('\u{2022}'));
    }

    #[test]
    fn test_notdef() {
        assert_eq!(glyph_name_to_unicode(b".notdef"), None);
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
        assert_eq!(glyph_name_to_unicode(b"ff"), Some('\u{FB00}'));
        assert_eq!(glyph_name_to_unicode(b"ffi"), Some('\u{FB03}'));
        assert_eq!(glyph_name_to_unicode(b"ffl"), Some('\u{FB04}'));
        assert_eq!(glyph_name_to_unicode(b"oneeighth"), Some('\u{215B}'));
        assert_eq!(glyph_name_to_unicode(b"nbspace"), Some('\u{00A0}'));
        assert_eq!(glyph_name_to_unicode(b"softhyphen"), Some('\u{00AD}'));
        assert_eq!(glyph_name_to_unicode(b"threeeighths"), Some('\u{215C}'));
        assert_eq!(glyph_name_to_unicode(b"fiveeighths"), Some('\u{215D}'));
        assert_eq!(glyph_name_to_unicode(b"seveneighths"), Some('\u{215E}'));
        assert_eq!(glyph_name_to_unicode(b"onethird"), Some('\u{2153}'));
        assert_eq!(glyph_name_to_unicode(b"twothirds"), Some('\u{2154}'));
        assert_eq!(glyph_name_to_unicode(b"colonmonetary"), Some('\u{20A1}'));
        assert_eq!(glyph_name_to_unicode(b"rupiah"), Some('\u{20A8}'));
    }
}
