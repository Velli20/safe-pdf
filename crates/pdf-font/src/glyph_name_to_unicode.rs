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
/// 5. **Zapf Dingbats table** — the `aN` names used by the ZapfDingbats font
///    (e.g. `"a109"` → `'♠'`).
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

    // Rule 4: static AGL subset table, then rule 5: Zapf Dingbats names
    lookup(AGL_TABLE, name).or_else(|| lookup(ZAPF_DINGBATS_TABLE, name))
}

/// Binary-searches a table sorted by glyph name.
fn lookup(table: &[(&[u8], char)], name: &[u8]) -> Option<char> {
    table
        .binary_search_by_key(&name, |&(n, _)| n)
        .ok()
        .and_then(|i| table.get(i))
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

/// Zapf Dingbats glyph names from the Adobe `zapfdingbats.txt` list.
///
/// The table is sorted lexicographically (byte order) for `binary_search_by_key`.
const ZAPF_DINGBATS_TABLE: &[(&[u8], char)] = &[
    (b"a1", '\u{2701}'),
    (b"a10", '\u{2721}'),
    (b"a100", '\u{275E}'),
    (b"a101", '\u{2761}'),
    (b"a102", '\u{2762}'),
    (b"a103", '\u{2763}'),
    (b"a104", '\u{2764}'),
    (b"a105", '\u{2710}'),
    (b"a106", '\u{2765}'),
    (b"a107", '\u{2766}'),
    (b"a108", '\u{2767}'),
    (b"a109", '\u{2660}'),
    (b"a11", '\u{261B}'),
    (b"a110", '\u{2665}'),
    (b"a111", '\u{2666}'),
    (b"a112", '\u{2663}'),
    (b"a117", '\u{2709}'),
    (b"a118", '\u{2708}'),
    (b"a119", '\u{2707}'),
    (b"a12", '\u{261E}'),
    (b"a120", '\u{2460}'),
    (b"a121", '\u{2461}'),
    (b"a122", '\u{2462}'),
    (b"a123", '\u{2463}'),
    (b"a124", '\u{2464}'),
    (b"a125", '\u{2465}'),
    (b"a126", '\u{2466}'),
    (b"a127", '\u{2467}'),
    (b"a128", '\u{2468}'),
    (b"a129", '\u{2469}'),
    (b"a13", '\u{270C}'),
    (b"a130", '\u{2776}'),
    (b"a131", '\u{2777}'),
    (b"a132", '\u{2778}'),
    (b"a133", '\u{2779}'),
    (b"a134", '\u{277A}'),
    (b"a135", '\u{277B}'),
    (b"a136", '\u{277C}'),
    (b"a137", '\u{277D}'),
    (b"a138", '\u{277E}'),
    (b"a139", '\u{277F}'),
    (b"a14", '\u{270D}'),
    (b"a140", '\u{2780}'),
    (b"a141", '\u{2781}'),
    (b"a142", '\u{2782}'),
    (b"a143", '\u{2783}'),
    (b"a144", '\u{2784}'),
    (b"a145", '\u{2785}'),
    (b"a146", '\u{2786}'),
    (b"a147", '\u{2787}'),
    (b"a148", '\u{2788}'),
    (b"a149", '\u{2789}'),
    (b"a15", '\u{270E}'),
    (b"a150", '\u{278A}'),
    (b"a151", '\u{278B}'),
    (b"a152", '\u{278C}'),
    (b"a153", '\u{278D}'),
    (b"a154", '\u{278E}'),
    (b"a155", '\u{278F}'),
    (b"a156", '\u{2790}'),
    (b"a157", '\u{2791}'),
    (b"a158", '\u{2792}'),
    (b"a159", '\u{2793}'),
    (b"a16", '\u{270F}'),
    (b"a160", '\u{2794}'),
    (b"a161", '\u{2192}'),
    (b"a162", '\u{27A3}'),
    (b"a163", '\u{2194}'),
    (b"a164", '\u{2195}'),
    (b"a165", '\u{2799}'),
    (b"a166", '\u{279B}'),
    (b"a167", '\u{279C}'),
    (b"a168", '\u{279D}'),
    (b"a169", '\u{279E}'),
    (b"a17", '\u{2711}'),
    (b"a170", '\u{279F}'),
    (b"a171", '\u{27A0}'),
    (b"a172", '\u{27A1}'),
    (b"a173", '\u{27A2}'),
    (b"a174", '\u{27A4}'),
    (b"a175", '\u{27A5}'),
    (b"a176", '\u{27A6}'),
    (b"a177", '\u{27A7}'),
    (b"a178", '\u{27A8}'),
    (b"a179", '\u{27A9}'),
    (b"a18", '\u{2712}'),
    (b"a180", '\u{27AB}'),
    (b"a181", '\u{27AD}'),
    (b"a182", '\u{27AF}'),
    (b"a183", '\u{27B2}'),
    (b"a184", '\u{27B3}'),
    (b"a185", '\u{27B5}'),
    (b"a186", '\u{27B8}'),
    (b"a187", '\u{27BA}'),
    (b"a188", '\u{27BB}'),
    (b"a189", '\u{27BC}'),
    (b"a19", '\u{2713}'),
    (b"a190", '\u{27BD}'),
    (b"a191", '\u{27BE}'),
    (b"a192", '\u{279A}'),
    (b"a193", '\u{27AA}'),
    (b"a194", '\u{27B6}'),
    (b"a195", '\u{27B9}'),
    (b"a196", '\u{2798}'),
    (b"a197", '\u{27B4}'),
    (b"a198", '\u{27B7}'),
    (b"a199", '\u{27AC}'),
    (b"a2", '\u{2702}'),
    (b"a20", '\u{2714}'),
    (b"a200", '\u{27AE}'),
    (b"a201", '\u{27B1}'),
    (b"a202", '\u{2703}'),
    (b"a203", '\u{2750}'),
    (b"a204", '\u{2752}'),
    (b"a205", '\u{276E}'),
    (b"a206", '\u{2770}'),
    (b"a21", '\u{2715}'),
    (b"a22", '\u{2716}'),
    (b"a23", '\u{2717}'),
    (b"a24", '\u{2718}'),
    (b"a25", '\u{2719}'),
    (b"a26", '\u{271A}'),
    (b"a27", '\u{271B}'),
    (b"a28", '\u{271C}'),
    (b"a29", '\u{2722}'),
    (b"a3", '\u{2704}'),
    (b"a30", '\u{2723}'),
    (b"a31", '\u{2724}'),
    (b"a32", '\u{2725}'),
    (b"a33", '\u{2726}'),
    (b"a34", '\u{2727}'),
    (b"a35", '\u{2605}'),
    (b"a36", '\u{2729}'),
    (b"a37", '\u{272A}'),
    (b"a38", '\u{272B}'),
    (b"a39", '\u{272C}'),
    (b"a4", '\u{260E}'),
    (b"a40", '\u{272D}'),
    (b"a41", '\u{272E}'),
    (b"a42", '\u{272F}'),
    (b"a43", '\u{2730}'),
    (b"a44", '\u{2731}'),
    (b"a45", '\u{2732}'),
    (b"a46", '\u{2733}'),
    (b"a47", '\u{2734}'),
    (b"a48", '\u{2735}'),
    (b"a49", '\u{2736}'),
    (b"a5", '\u{2706}'),
    (b"a50", '\u{2737}'),
    (b"a51", '\u{2738}'),
    (b"a52", '\u{2739}'),
    (b"a53", '\u{273A}'),
    (b"a54", '\u{273B}'),
    (b"a55", '\u{273C}'),
    (b"a56", '\u{273D}'),
    (b"a57", '\u{273E}'),
    (b"a58", '\u{273F}'),
    (b"a59", '\u{2740}'),
    (b"a6", '\u{271D}'),
    (b"a60", '\u{2741}'),
    (b"a61", '\u{2742}'),
    (b"a62", '\u{2743}'),
    (b"a63", '\u{2744}'),
    (b"a64", '\u{2745}'),
    (b"a65", '\u{2746}'),
    (b"a66", '\u{2747}'),
    (b"a67", '\u{2748}'),
    (b"a68", '\u{2749}'),
    (b"a69", '\u{274A}'),
    (b"a7", '\u{271E}'),
    (b"a70", '\u{274B}'),
    (b"a71", '\u{25CF}'),
    (b"a72", '\u{274D}'),
    (b"a73", '\u{25A0}'),
    (b"a74", '\u{274F}'),
    (b"a75", '\u{2751}'),
    (b"a76", '\u{25B2}'),
    (b"a77", '\u{25BC}'),
    (b"a78", '\u{25C6}'),
    (b"a79", '\u{2756}'),
    (b"a8", '\u{271F}'),
    (b"a81", '\u{25D7}'),
    (b"a82", '\u{2758}'),
    (b"a83", '\u{2759}'),
    (b"a84", '\u{275A}'),
    (b"a85", '\u{276F}'),
    (b"a86", '\u{2771}'),
    (b"a87", '\u{2772}'),
    (b"a88", '\u{2773}'),
    (b"a89", '\u{2768}'),
    (b"a9", '\u{2720}'),
    (b"a90", '\u{2769}'),
    (b"a91", '\u{276C}'),
    (b"a92", '\u{276D}'),
    (b"a93", '\u{276A}'),
    (b"a94", '\u{276B}'),
    (b"a95", '\u{2774}'),
    (b"a96", '\u{2775}'),
    (b"a97", '\u{275B}'),
    (b"a98", '\u{275C}'),
    (b"a99", '\u{275D}'),
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
