//! The glyph order the TrueType core fonts share beyond the standard Macintosh glyph order.
//!
//! Arial, Times New Roman, Courier New and Tahoma keep the standard Macintosh order for their
//! first 258 glyphs and continue with the same Latin Extended, Greek and Hebrew glyphs after it.
//! A PDF that names one of these fonts without embedding it, and selects glyphs by index, relies
//! on that order.

/// Runs of consecutive glyph indices the core fonts share from index 258 on: the first index of
/// each run and the characters of its glyphs in index order.
pub(crate) const CORE_FONT_GLYPH_RUNS: [(u16, &[char]); 3] = [
    (
        258,
        &[
            '\u{0102}', '\u{0103}', '\u{0104}', '\u{0105}', '\u{010E}', '\u{010F}', '\u{0110}',
            '\u{0118}', '\u{0119}', '\u{011A}', '\u{011B}', '\u{0139}', '\u{013A}', '\u{013D}',
            '\u{013E}', '\u{013F}', '\u{0140}', '\u{0143}', '\u{0144}', '\u{0147}', '\u{0148}',
            '\u{0150}', '\u{0151}', '\u{0154}', '\u{0155}', '\u{0158}', '\u{0159}', '\u{015A}',
            '\u{015B}', '\u{0162}', '\u{0163}', '\u{0164}', '\u{0165}', '\u{016E}', '\u{016F}',
            '\u{0170}', '\u{0171}', '\u{0179}', '\u{017A}', '\u{017B}', '\u{017C}', '\u{0393}',
            '\u{0398}', '\u{03A6}', '\u{03B1}', '\u{03B4}', '\u{03B5}', '\u{03C3}', '\u{03C4}',
            '\u{03C6}', '\u{2017}', '\u{203C}', '\u{207F}', '\u{20A7}',
        ],
    ),
    (
        653,
        &[
            '\u{05B0}', '\u{05B1}', '\u{05B2}', '\u{05B3}', '\u{05B4}', '\u{05B5}', '\u{05B6}',
            '\u{05B7}', '\u{05B8}', '\u{05B9}', '\u{05BB}', '\u{05BC}', '\u{05BD}', '\u{05BE}',
            '\u{05BF}', '\u{05C0}', '\u{05C1}', '\u{05C2}', '\u{05C3}', '\u{05D0}', '\u{05D1}',
            '\u{05D2}', '\u{05D3}', '\u{05D4}', '\u{05D5}', '\u{05D6}', '\u{05D7}', '\u{05D8}',
            '\u{05D9}', '\u{05DA}', '\u{05DB}', '\u{05DC}', '\u{05DD}', '\u{05DE}', '\u{05DF}',
            '\u{05E0}', '\u{05E1}', '\u{05E2}', '\u{05E3}', '\u{05E4}', '\u{05E5}', '\u{05E6}',
            '\u{05E7}', '\u{05E8}', '\u{05E9}', '\u{05EA}', '\u{05F0}', '\u{05F1}', '\u{05F2}',
            '\u{05F3}', '\u{05F4}', '\u{20AA}',
        ],
    ),
    (
        710,
        &[
            '\u{FB20}', '\u{FB2A}', '\u{FB2B}', '\u{FB2C}', '\u{FB2D}', '\u{FB2E}', '\u{FB2F}',
            '\u{FB30}', '\u{FB31}', '\u{FB32}', '\u{FB33}', '\u{FB34}', '\u{FB35}', '\u{FB36}',
            '\u{FB38}', '\u{FB39}', '\u{FB3A}', '\u{FB3B}', '\u{FB3C}', '\u{FB3E}', '\u{FB40}',
            '\u{FB41}', '\u{FB43}', '\u{FB44}', '\u{FB46}', '\u{FB47}', '\u{FB48}', '\u{FB49}',
            '\u{FB4A}', '\u{FB4B}', '\u{FB4C}', '\u{FB4D}', '\u{FB4E}', '\u{FB4F}', '\u{200C}',
            '\u{200D}', '\u{200E}', '\u{200F}',
        ],
    ),
];
