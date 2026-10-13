//! Best-effort CID-to-Unicode mappings for substituted CID fonts.

use std::{collections::HashMap, sync::Arc};

/// Best-effort CID-to-Unicode mapping used when a CID font has no `/ToUnicode` entry.
#[derive(Clone)]
pub enum CidToUnicode {
    /// Inverted predefined Unicode CMap of a CJK character collection.
    Collection(Arc<HashMap<u16, char>>),
    /// CIDs are glyph indices in the standard Macintosh glyph order.
    ///
    /// TrueType core fonts such as Arial, Times New Roman, and Courier New keep this order for
    /// their first 258 glyphs. A non-embedded Identity CIDFontType2 font naming one of them
    /// selects glyphs of that font by index, so its CIDs follow this order.
    StandardGlyphOrder,
}

impl CidToUnicode {
    /// Returns the Unicode value of `cid`, if known.
    #[must_use]
    pub fn get(&self, cid: u16) -> Option<char> {
        match self {
            Self::Collection(map) => map.get(&cid).copied(),
            Self::StandardGlyphOrder => STANDARD_GLYPH_ORDER_UNICODE
                .get(usize::from(cid))
                .copied()
                .flatten(),
        }
    }
}

/// Unicode value of each glyph in the standard Macintosh glyph order, by glyph index.
const STANDARD_GLYPH_ORDER_UNICODE: [Option<char>; 258] = [
    None,             // .notdef
    None,             // .null
    None,             // nonmarkingreturn
    Some(' '),        // space
    Some('!'),        // exclam
    Some('"'),        // quotedbl
    Some('#'),        // numbersign
    Some('$'),        // dollar
    Some('%'),        // percent
    Some('&'),        // ampersand
    Some('\''),       // quotesingle
    Some('('),        // parenleft
    Some(')'),        // parenright
    Some('*'),        // asterisk
    Some('+'),        // plus
    Some(','),        // comma
    Some('-'),        // hyphen
    Some('.'),        // period
    Some('/'),        // slash
    Some('0'),        // zero
    Some('1'),        // one
    Some('2'),        // two
    Some('3'),        // three
    Some('4'),        // four
    Some('5'),        // five
    Some('6'),        // six
    Some('7'),        // seven
    Some('8'),        // eight
    Some('9'),        // nine
    Some(':'),        // colon
    Some(';'),        // semicolon
    Some('<'),        // less
    Some('='),        // equal
    Some('>'),        // greater
    Some('?'),        // question
    Some('@'),        // at
    Some('A'),        // A
    Some('B'),        // B
    Some('C'),        // C
    Some('D'),        // D
    Some('E'),        // E
    Some('F'),        // F
    Some('G'),        // G
    Some('H'),        // H
    Some('I'),        // I
    Some('J'),        // J
    Some('K'),        // K
    Some('L'),        // L
    Some('M'),        // M
    Some('N'),        // N
    Some('O'),        // O
    Some('P'),        // P
    Some('Q'),        // Q
    Some('R'),        // R
    Some('S'),        // S
    Some('T'),        // T
    Some('U'),        // U
    Some('V'),        // V
    Some('W'),        // W
    Some('X'),        // X
    Some('Y'),        // Y
    Some('Z'),        // Z
    Some('['),        // bracketleft
    Some('\\'),       // backslash
    Some(']'),        // bracketright
    Some('^'),        // asciicircum
    Some('_'),        // underscore
    Some('`'),        // grave
    Some('a'),        // a
    Some('b'),        // b
    Some('c'),        // c
    Some('d'),        // d
    Some('e'),        // e
    Some('f'),        // f
    Some('g'),        // g
    Some('h'),        // h
    Some('i'),        // i
    Some('j'),        // j
    Some('k'),        // k
    Some('l'),        // l
    Some('m'),        // m
    Some('n'),        // n
    Some('o'),        // o
    Some('p'),        // p
    Some('q'),        // q
    Some('r'),        // r
    Some('s'),        // s
    Some('t'),        // t
    Some('u'),        // u
    Some('v'),        // v
    Some('w'),        // w
    Some('x'),        // x
    Some('y'),        // y
    Some('z'),        // z
    Some('{'),        // braceleft
    Some('|'),        // bar
    Some('}'),        // braceright
    Some('~'),        // asciitilde
    Some('\u{00C4}'), // Adieresis
    Some('\u{00C5}'), // Aring
    Some('\u{00C7}'), // Ccedilla
    Some('\u{00C9}'), // Eacute
    Some('\u{00D1}'), // Ntilde
    Some('\u{00D6}'), // Odieresis
    Some('\u{00DC}'), // Udieresis
    Some('\u{00E1}'), // aacute
    Some('\u{00E0}'), // agrave
    Some('\u{00E2}'), // acircumflex
    Some('\u{00E4}'), // adieresis
    Some('\u{00E3}'), // atilde
    Some('\u{00E5}'), // aring
    Some('\u{00E7}'), // ccedilla
    Some('\u{00E9}'), // eacute
    Some('\u{00E8}'), // egrave
    Some('\u{00EA}'), // ecircumflex
    Some('\u{00EB}'), // edieresis
    Some('\u{00ED}'), // iacute
    Some('\u{00EC}'), // igrave
    Some('\u{00EE}'), // icircumflex
    Some('\u{00EF}'), // idieresis
    Some('\u{00F1}'), // ntilde
    Some('\u{00F3}'), // oacute
    Some('\u{00F2}'), // ograve
    Some('\u{00F4}'), // ocircumflex
    Some('\u{00F6}'), // odieresis
    Some('\u{00F5}'), // otilde
    Some('\u{00FA}'), // uacute
    Some('\u{00F9}'), // ugrave
    Some('\u{00FB}'), // ucircumflex
    Some('\u{00FC}'), // udieresis
    Some('\u{2020}'), // dagger
    Some('\u{00B0}'), // degree
    Some('\u{00A2}'), // cent
    Some('\u{00A3}'), // sterling
    Some('\u{00A7}'), // section
    Some('\u{2022}'), // bullet
    Some('\u{00B6}'), // paragraph
    Some('\u{00DF}'), // germandbls
    Some('\u{00AE}'), // registered
    Some('\u{00A9}'), // copyright
    Some('\u{2122}'), // trademark
    Some('\u{00B4}'), // acute
    Some('\u{00A8}'), // dieresis
    Some('\u{2260}'), // notequal
    Some('\u{00C6}'), // AE
    Some('\u{00D8}'), // Oslash
    Some('\u{221E}'), // infinity
    Some('\u{00B1}'), // plusminus
    Some('\u{2264}'), // lessequal
    Some('\u{2265}'), // greaterequal
    Some('\u{00A5}'), // yen
    Some('\u{00B5}'), // mu
    Some('\u{2202}'), // partialdiff
    Some('\u{2211}'), // summation
    Some('\u{220F}'), // product
    Some('\u{03C0}'), // pi
    Some('\u{222B}'), // integral
    Some('\u{00AA}'), // ordfeminine
    Some('\u{00BA}'), // ordmasculine
    Some('\u{2126}'), // Omega
    Some('\u{00E6}'), // ae
    Some('\u{00F8}'), // oslash
    Some('\u{00BF}'), // questiondown
    Some('\u{00A1}'), // exclamdown
    Some('\u{00AC}'), // logicalnot
    Some('\u{221A}'), // radical
    Some('\u{0192}'), // florin
    Some('\u{2248}'), // approxequal
    Some('\u{2206}'), // Delta
    Some('\u{00AB}'), // guillemotleft
    Some('\u{00BB}'), // guillemotright
    Some('\u{2026}'), // ellipsis
    Some('\u{00A0}'), // nonbreakingspace
    Some('\u{00C0}'), // Agrave
    Some('\u{00C3}'), // Atilde
    Some('\u{00D5}'), // Otilde
    Some('\u{0152}'), // OE
    Some('\u{0153}'), // oe
    Some('\u{2013}'), // endash
    Some('\u{2014}'), // emdash
    Some('\u{201C}'), // quotedblleft
    Some('\u{201D}'), // quotedblright
    Some('\u{2018}'), // quoteleft
    Some('\u{2019}'), // quoteright
    Some('\u{00F7}'), // divide
    Some('\u{25CA}'), // lozenge
    Some('\u{00FF}'), // ydieresis
    Some('\u{0178}'), // Ydieresis
    Some('\u{2044}'), // fraction
    Some('\u{00A4}'), // currency
    Some('\u{2039}'), // guilsinglleft
    Some('\u{203A}'), // guilsinglright
    Some('\u{FB01}'), // fi
    Some('\u{FB02}'), // fl
    Some('\u{2021}'), // daggerdbl
    Some('\u{00B7}'), // periodcentered
    Some('\u{201A}'), // quotesinglbase
    Some('\u{201E}'), // quotedblbase
    Some('\u{2030}'), // perthousand
    Some('\u{00C2}'), // Acircumflex
    Some('\u{00CA}'), // Ecircumflex
    Some('\u{00C1}'), // Aacute
    Some('\u{00CB}'), // Edieresis
    Some('\u{00C8}'), // Egrave
    Some('\u{00CD}'), // Iacute
    Some('\u{00CE}'), // Icircumflex
    Some('\u{00CF}'), // Idieresis
    Some('\u{00CC}'), // Igrave
    Some('\u{00D3}'), // Oacute
    Some('\u{00D4}'), // Ocircumflex
    Some('\u{F8FF}'), // apple
    Some('\u{00D2}'), // Ograve
    Some('\u{00DA}'), // Uacute
    Some('\u{00DB}'), // Ucircumflex
    Some('\u{00D9}'), // Ugrave
    Some('\u{0131}'), // dotlessi
    Some('\u{02C6}'), // circumflex
    Some('\u{02DC}'), // tilde
    Some('\u{00AF}'), // macron
    Some('\u{02D8}'), // breve
    Some('\u{02D9}'), // dotaccent
    Some('\u{02DA}'), // ring
    Some('\u{00B8}'), // cedilla
    Some('\u{02DD}'), // hungarumlaut
    Some('\u{02DB}'), // ogonek
    Some('\u{02C7}'), // caron
    Some('\u{0141}'), // Lslash
    Some('\u{0142}'), // lslash
    Some('\u{0160}'), // Scaron
    Some('\u{0161}'), // scaron
    Some('\u{017D}'), // Zcaron
    Some('\u{017E}'), // zcaron
    Some('\u{00A6}'), // brokenbar
    Some('\u{00D0}'), // Eth
    Some('\u{00F0}'), // eth
    Some('\u{00DD}'), // Yacute
    Some('\u{00FD}'), // yacute
    Some('\u{00DE}'), // Thorn
    Some('\u{00FE}'), // thorn
    Some('\u{2212}'), // minus
    Some('\u{00D7}'), // multiply
    Some('\u{00B9}'), // onesuperior
    Some('\u{00B2}'), // twosuperior
    Some('\u{00B3}'), // threesuperior
    Some('\u{00BD}'), // onehalf
    Some('\u{00BC}'), // onequarter
    Some('\u{00BE}'), // threequarters
    Some('\u{20A3}'), // franc
    Some('\u{011E}'), // Gbreve
    Some('\u{011F}'), // gbreve
    Some('\u{0130}'), // Idotaccent
    Some('\u{015E}'), // Scedilla
    Some('\u{015F}'), // scedilla
    Some('\u{0106}'), // Cacute
    Some('\u{0107}'), // cacute
    Some('\u{010C}'), // Ccaron
    Some('\u{010D}'), // ccaron
    Some('\u{0111}'), // dcroat
];
