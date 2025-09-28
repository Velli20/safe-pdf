use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

use thiserror::Error;

use crate::cff::{
    cursor::{Cursor, CursorReadError},
    parser::read_encoded_int,
};

/// Represents a parsed Top DICT entry from a CFF font.
#[derive(Default)]
pub(crate) struct TopDictEntry {
    /// Offset to the `CharStrings` INDEX, where each glyph program resides.
    pub char_strings_offset: Option<u16>,
    /// Charset reference: either an offset to a charset table or a predefined id.
    pub charset_offset: Option<u16>,
    /// Encoding reference: either an offset to a custom encoding table or a
    /// predefined id.
    pub encoding: Option<u16>,
}

/// Top DICT operators defined by the Compact Font Format (CFF) specification.
#[derive(Clone, Copy, PartialEq, Debug, FromPrimitive)]
enum TopDictOperator {
    /// Name of the font in the source font program.
    Version = 0,
    /// Trademark / legal notice string.
    Notice,
    /// Full PostScript font name.
    FullName,
    /// Family name portion.
    FamilyName,
    /// Weight descriptor.
    Weight,
    /// Font bounding box.
    FontBBox,
    /// UniqueID
    UniqueID = 13,
    /// XUID array.
    Xuid,
    /// Offset to the charset table (or 0 / 1 / 2 for predefined charsets).
    Charset = 15,
    /// Offset to the encoding table (or 0 / 1 for predefined encodings).
    Encoding,
    /// Offset to the CharStrings INDEX (glyph programs).
    CharStrings,
    /// (size, offset) pair pointing to the Private DICT data.
    Private,
    /// Copyright notice (escaped operator).
    Copyright = (12 << 8),
    /// Boolean (0 / 1) indicating fixed / monospaced advance widths.
    IsFixedPitch,
    /// Italic angle in degrees counter‑clockwise from the vertical.
    ItalicAngle,
    /// Underline position (baseline offset, typically negative).
    UnderlinePosition,
    /// Underline thickness.
    UnderlineThickness,
    /// Paint type (0 = filled, 1 = stroked) for Type 1 compatibility.
    PaintType,
    /// CharstringType (1 for Type 1 style charstrings, 2 for CFF2).
    CharstringType,
    /// Font transformation matrix (array of 6 numbers) mapping font
    /// units to user space. Usually omitted (defaults to identity * 0.001).
    FontMatrix,
    /// Stroke width (only meaningful for stroked / PaintType=1 fonts).
    StrokeWidth,
    /// Synthetic base font index (used for synthetic fonts). Rare.
    SyntheticBase = (12 << 8 | 20),
    /// PostScript language source (string) for CID / synthetic fonts.
    PostScript,
    /// Base font name used in synthetic blending contexts.
    BaseFontName,
    /// Base font blend data (array) for multiple-master style blending.
    BaseFontBlend,
    /// ROS (Registry, Ordering, Supplement) triple for CID-keyed fonts.
    RegistryOrderingSupplement = (12 << 8 | 30),
    /// CIDFontVersion value.
    CIDFontVersion,
    /// CIDFontRevision value.
    CIDFontRevision,
    /// CIDFontType (e.g. 0 = Type 0, 2 = CID-keyed Type 2 charstrings).
    CIDFontType,
    /// CIDCount (upper bound on CID values defined in the font).
    CIDCount,
    /// UIDBase for constructing unique identifiers (optional).
    UIDBase,
    /// Offset to FDArray INDEX (required for CID-keyed fonts).
    FDArray,
    /// Offset to FDSelect data (maps glyphs to Font DICTs).
    FDSelect,
    /// PostScript FontName.
    FontName,
}

/// CFF DICT token: either a number or an operator (op).
#[derive(Debug, Clone)]
pub(crate) enum DictToken {
    Number(i32),
    Operator(u16),
}

/// Errors that can occur while reading / interpreting the Top DICT.
#[derive(Debug, Error)]
pub enum TopDictReadError {
    #[error("Missing operand for operator 0x{0:04X}")]
    MissingOperand(u16),
    #[error("Operand type mismatch for operator 0x{0:04X} (expected number)")]
    OperandTypeMismatch(u16),
    #[error("Operand value out of range for operator 0x{0:04X}")]
    OperandValueOutOfRange(u16),
    #[error("Unexpected operator for Top DICT")]
    UnexpectedOperator(u16),
    #[error("Unsupported real number format")]
    UnsupportedRealNumber,
    #[error("Unexpected DICT byte: {0}")]
    UnexpectedDictByte(u8),
    #[error("Cursor read error: {0}")]
    CursorReadError(#[from] CursorReadError),
}

fn parse_dict(cur: &mut Cursor) -> Result<Vec<DictToken>, TopDictReadError> {
    let mut out = Vec::new();

    while cur.pos() < cur.len() {
        let b = cur.read_u8()?;
        let token = match b {
            0..=21 => {
                if b == 12 {
                    let b2 = cur.read_u8()?;
                    DictToken::Operator(((u16::from(b)) << 8) | u16::from(b2))
                } else {
                    DictToken::Operator(u16::from(b))
                }
            }
            28 | 32..=254 => {
                let v = read_encoded_int(cur, b)?;
                DictToken::Number(v)
            }
            29 => {
                let s = cur.read_n(4)?;
                let val = i32::from_be_bytes([s[0], s[1], s[2], s[3]]);
                DictToken::Number(val)
            }
            30 => {
                return Err(TopDictReadError::UnsupportedRealNumber);
            }
            _ => return Err(TopDictReadError::UnexpectedDictByte(b)),
        };
        out.push(token);
    }
    Ok(out)
}

impl TopDictEntry {
    /// Constructs a `TopDictEntry` by interpreting a linear sequence of
    /// `DictToken`s that represent the Top DICT (Type 1 / CFF) key/value pairs.
    ///
    /// # Parameters
    ///
    ///  - `operators`: A slice of `DictToken`s representing the parsed DICT data.
    ///
    /// # Returns
    ///
    /// Returns a partially populated `TopDictEntry`; fields left as `None`
    /// indicate that the corresponding operator was not present.
    fn from_dictionary_tokens(operators: &[DictToken]) -> Result<TopDictEntry, TopDictReadError> {
        let mut stack: Vec<DictToken> = Vec::new();
        let mut top_dictionary = TopDictEntry::default();

        for op in operators {
            match op {
                DictToken::Operator(raw) => {
                    let Some(opcode) = TopDictOperator::from_u16(*raw) else {
                        return Err(TopDictReadError::UnexpectedOperator(*raw));
                    };

                    match opcode {
                        TopDictOperator::Encoding
                        | TopDictOperator::Charset
                        | TopDictOperator::CharStrings => {
                            // Pop one number operand
                            let token =
                                stack.pop().ok_or(TopDictReadError::MissingOperand(*raw))?;
                            let number = match token {
                                DictToken::Number(v) => v,
                                _ => return Err(TopDictReadError::OperandTypeMismatch(*raw)),
                            };
                            let offset = u16::try_from(number)
                                .map_err(|_| TopDictReadError::OperandValueOutOfRange(*raw))?;
                            match opcode {
                                TopDictOperator::Encoding => top_dictionary.encoding = Some(offset),
                                TopDictOperator::Charset => {
                                    top_dictionary.charset_offset = Some(offset)
                                }
                                TopDictOperator::CharStrings => {
                                    top_dictionary.char_strings_offset = Some(offset)
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
                // Operand, push on the stack
                token => stack.push(token.clone()),
            }
        }

        Ok(top_dictionary)
    }

    pub(crate) fn read(data: &[u8]) -> Result<TopDictEntry, TopDictReadError> {
        let mut cur = Cursor::new(data);
        let tokens = parse_dict(&mut cur)?;
        TopDictEntry::from_dictionary_tokens(&tokens)
    }
}
