use thiserror::Error;

use crate::cff::{
    char_string_interpreter::CharStringOperatorTrait, cursor::Cursor, parser::parse_int,
};

/// Mask to identify two-byte operators (first byte is 12).
const TWO_BYTE_OP_MASK: u16 = 12 << 8;

/// Error variants that may occur while evaluating a Type 2 CharString operator.
#[derive(Debug, Error)]
pub enum CharStringReadError {
    #[error("Cursor read error: {0}")]
    UnexpectedOperator(u8),
    #[error("Cursor read error: {0}")]
    CursorReadError(#[from] crate::cff::cursor::CursorReadError),
    #[error("Unexpected two-byte operator: {0}")]
    UnexpectedTwoByteOperator(u8),
}

/// Horizontal stem hints: consumes pairs of (y, dy). Variadic by pairs.
#[derive(Default)]
pub struct HStemOp;

impl HStemOp {
    const OPCODE: u16 = 1;
}

/// Vertical stem hints: consumes pairs of (x, dx). Variadic by pairs.
#[derive(Default)]
pub struct VStemOp;

impl VStemOp {
    const OPCODE: u16 = 3;
}

/// Move current point vertically by dy. First op may include width.
#[derive(Default)]
pub struct VMoveToOp;

impl VMoveToOp {
    const OPCODE: u16 = 4;
}

/// Draw one or more relative lines: pairs of (dx, dy) per segment.
#[derive(Default)]
pub struct RLineToOp;

impl RLineToOp {
    const OPCODE: u16 = 5;
}

/// Draw alternating horizontal/vertical segments starting with horizontal.
#[derive(Default)]
pub struct HLineToOp;

impl HLineToOp {
    const OPCODE: u16 = 6;
}

/// Draw alternating vertical/horizontal segments starting with vertical.
#[derive(Default)]
pub struct VLineToOp;

impl VLineToOp {
    const OPCODE: u16 = 7;
}

/// Draw one or more cubic Bézier curves with relative control points: groups of 6.
#[derive(Default)]
pub struct RRCurveToOp;

impl RRCurveToOp {
    const OPCODE: u16 = 8;
}

/// Call local subroutine: pops subroutine index.
#[derive(Default)]
pub struct CallSubroutineOp;

impl CallSubroutineOp {
    const OPCODE: u16 = 10;
}

/// Return from subroutine.
#[derive(Default)]
pub struct ReturnOp;
impl ReturnOp {
    const OPCODE: u16 = 11;
}

/// End the charstring.
#[derive(Default)]
pub struct EndCharOp;
impl EndCharOp {
    const OPCODE: u16 = 14;
}

/// Horizontal stem hints (like hstem) that may be followed by hintmask.
#[derive(Default)]
pub struct HStemHmOp;

impl HStemHmOp {
    const OPCODE: u16 = 18;
}

/// Hint mask: consumes pending stem hint operands, then reads mask bytes from stream.
#[derive(Default)]
pub struct HintMaskOp;

impl HintMaskOp {
    const OPCODE: u16 = 19;
}

/// Counter mask: like hintmask but for counter-controlled hints.
#[derive(Default)]
pub struct CntrMaskOp;

impl CntrMaskOp {
    const OPCODE: u16 = 20;
}

/// Move current point by (dx, dy). First op may include width.
#[derive(Default)]
pub struct RMoveToOp;

impl RMoveToOp {
    const OPCODE: u16 = 21;
}

/// Move current point horizontally by dx. First op may include width.
#[derive(Default)]
pub struct HMoveToOp;

impl HMoveToOp {
    const OPCODE: u16 = 22;
}

/// Vertical stem hints (like vstem) that may be followed by hintmask.
#[derive(Default)]
pub struct VStemHmOp;

impl VStemHmOp {
    const OPCODE: u16 = 23;
}

/// One or more curves followed by a final line segment: 6n + 2 operands.
#[derive(Default)]
pub struct RCurveLineOp;

impl RCurveLineOp {
    const OPCODE: u16 = 24;
}

/// One or more line segments followed by a final curve: 2n + 6 operands.
#[derive(Default)]
pub struct RLineCurveOp;

impl RLineCurveOp {
    const OPCODE: u16 = 25;
}

/// Vertical-vertical curve segments. Variadic; primarily vertical tangents.
#[derive(Default)]
pub struct VVCurveToOp;

impl VVCurveToOp {
    const OPCODE: u16 = 26;
}

/// Horizontal-horizontal curve segments. Variadic; primarily horizontal tangents.
#[derive(Default)]
pub struct HHCurveToOp;

impl HHCurveToOp {
    const OPCODE: u16 = 27;
}
/// Call global subroutine: pops global subroutine index.
#[derive(Default)]
pub struct CallGSubrOp;

impl CallGSubrOp {
    const OPCODE: u16 = 29;
}

/// Vertical then horizontal curve segments. Variadic.
#[derive(Default)]
pub struct VHCurveToOp;

impl VHCurveToOp {
    const OPCODE: u16 = 30;
}

/// Horizontal then vertical curve segments. Variadic.
#[derive(Default)]
pub struct HVCurveToOp;

impl HVCurveToOp {
    const OPCODE: u16 = 31;
}

/// Deprecated dotsection operator (ignored in Type 2). No operands.
#[derive(Default)]
pub struct DotSectionOp;

impl DotSectionOp {
    const OPCODE: u16 = 12 << 8;
}

/// Logical AND: pops two integers, pushes result.
#[derive(Default)]
pub struct AndOp;

impl AndOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 3;
}

/// Logical OR: pops two integers, pushes result.
#[derive(Default)]
pub struct OrOp;

impl OrOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 4;
}

/// Logical NOT: pops one integer, pushes result.
#[derive(Default)]
pub struct NotOp;

impl NotOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 5;
}

/// Absolute value: pops one number, pushes abs(value).
#[derive(Default)]
pub struct AbsOp;

impl AbsOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 9;
}

/// Addition: pops two numbers, pushes sum.
#[derive(Default)]
pub struct AddOp;

impl AddOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 10;
}

/// Subtraction: pops two numbers, pushes difference.
#[derive(Default)]
pub struct SubOp;

impl SubOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 11;
}

/// Division: pops two numbers, pushes quotient.
#[derive(Default)]
pub struct DivOp;

impl DivOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 12;
}

/// Negation: pops one number, pushes -value.
#[derive(Default)]
pub struct NegOp;

impl NegOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 14;
}

/// Equality test: pops two numbers, pushes 1 if equal, else 0.
#[derive(Default)]
pub struct EqOp;

impl EqOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 15;
}

/// Drop: pops one element and discards it.
#[derive(Default)]
pub struct DropOp;

impl DropOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 18;
}

/// Put: pops (index, value) and stores value in transient array at index.
#[derive(Default)]
pub struct PutOp;

impl PutOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 20;
}

/// Get: pops (index) and pushes value from transient array.
#[derive(Default)]
pub struct GetOp;

impl GetOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 21;
}

/// If-else: pops (v1, v2, s1, s2) and pushes s1 if v1 <= v2, else s2.
#[derive(Default)]
pub struct IfElseOp;

impl IfElseOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 22;
}

/// Random: pushes a pseudorandom number on stack; consumes none.
#[derive(Default)]
pub struct RandomOp;

impl RandomOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 23;
}

/// Multiplication: pops two numbers, pushes product.
#[derive(Default)]
pub struct MulOp;

impl MulOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 24;
}

/// Square root: pops one number, pushes sqrt(value).
#[derive(Default)]
pub struct SqrtOp;

impl SqrtOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 26;
}

/// Dup: duplicates the top stack element; requires at least one element.
#[derive(Default)]
pub struct DupOp;

impl DupOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 27;
}

/// Exch: exchanges the top two stack elements.
#[derive(Default)]
pub struct ExchOp;

impl ExchOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 28;
}

/// Index: pops (n) and duplicates the nth element (0 = top).
#[derive(Default)]
pub struct IndexOp;

impl IndexOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 29;
}

/// Roll: pops (n, j) and rolls the top n elements by j positions.
#[derive(Default)]
pub struct RollOp;

impl RollOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 30;
}

/// hflex: draws a flexible curve with mostly horizontal tangents. Pops 7 numbers.
#[derive(Default)]
pub struct HFlexOp;

impl HFlexOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 34;
}

/// flex: draws a flexible curve. Pops 13 numbers.
#[derive(Default)]
pub struct FlexOp;

impl FlexOp {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 35;
}

/// hflex1: flexible curve variant. Pops 9 numbers.
#[derive(Default)]
pub struct HFlex1Op;

impl HFlex1Op {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 36;
}

/// flex1: flexible curve variant. Pops 11 numbers.
#[derive(Default)]
pub struct Flex1Op;

impl Flex1Op {
    const OPCODE: u16 = TWO_BYTE_OP_MASK | 37;
}

#[derive(Default)]
pub struct NumberOp {
    pub value: i32,
}

pub fn char_strings_from(
    data: &[u8],
) -> Result<Vec<Box<dyn CharStringOperatorTrait>>, CharStringReadError> {
    let mut ops = Vec::new();
    let mut cur = Cursor::new(data);

    while !cur.is_empty() {
        let b0 = cur.read_u8()?;
        let b_u16 = u16::from(b0);
        let op: Box<dyn CharStringOperatorTrait> = match b_u16 {
            0 | 2 | 9 | 13 | 15 | 16 | 17 => {
                return Err(CharStringReadError::UnexpectedOperator(b0));
            }
            28 | 32..=254 => {
                let v = parse_int(&mut cur, b0)?;
                Box::new(NumberOp { value: v })
            }

            HStemOp::OPCODE => Box::new(HStemOp),
            VStemOp::OPCODE => Box::new(VStemOp),
            VMoveToOp::OPCODE => Box::new(VMoveToOp),
            RLineToOp::OPCODE => Box::new(RLineToOp),
            HLineToOp::OPCODE => Box::new(HLineToOp),
            VLineToOp::OPCODE => Box::new(VLineToOp),
            RRCurveToOp::OPCODE => Box::new(RRCurveToOp),
            CallSubroutineOp::OPCODE => Box::new(CallSubroutineOp),
            ReturnOp::OPCODE => Box::new(ReturnOp),
            EndCharOp::OPCODE => Box::new(EndCharOp),
            HStemHmOp::OPCODE => Box::new(HStemHmOp),
            HintMaskOp::OPCODE => Box::new(HintMaskOp),
            CntrMaskOp::OPCODE => Box::new(CntrMaskOp),
            RMoveToOp::OPCODE => Box::new(RMoveToOp),
            HMoveToOp::OPCODE => Box::new(HMoveToOp),
            VStemHmOp::OPCODE => Box::new(VStemHmOp),
            RCurveLineOp::OPCODE => Box::new(RCurveLineOp),
            RLineCurveOp::OPCODE => Box::new(RLineCurveOp),
            VVCurveToOp::OPCODE => Box::new(VVCurveToOp),
            HHCurveToOp::OPCODE => Box::new(HHCurveToOp),
            CallGSubrOp::OPCODE => Box::new(CallGSubrOp),
            VHCurveToOp::OPCODE => Box::new(VHCurveToOp),
            HVCurveToOp::OPCODE => Box::new(HVCurveToOp),
            12 => {
                let b2 = cur.read_u8()?;
                let b2_u16 = u16::from(b2) << 8;
                match b2_u16 {
                    DotSectionOp::OPCODE => Box::new(DotSectionOp),
                    AndOp::OPCODE => Box::new(AndOp),
                    OrOp::OPCODE => Box::new(OrOp),
                    NotOp::OPCODE => Box::new(NotOp),
                    AbsOp::OPCODE => Box::new(AbsOp),
                    AddOp::OPCODE => Box::new(AddOp),
                    SubOp::OPCODE => Box::new(SubOp),
                    DivOp::OPCODE => Box::new(DivOp),
                    NegOp::OPCODE => Box::new(NegOp),
                    EqOp::OPCODE => Box::new(EqOp),
                    DropOp::OPCODE => Box::new(DropOp),
                    PutOp::OPCODE => Box::new(PutOp),
                    GetOp::OPCODE => Box::new(GetOp),
                    IfElseOp::OPCODE => Box::new(IfElseOp),
                    RandomOp::OPCODE => Box::new(RandomOp),
                    MulOp::OPCODE => Box::new(MulOp),
                    SqrtOp::OPCODE => Box::new(SqrtOp),
                    DupOp::OPCODE => Box::new(DupOp),
                    ExchOp::OPCODE => Box::new(ExchOp),
                    IndexOp::OPCODE => Box::new(IndexOp),
                    RollOp::OPCODE => Box::new(RollOp),
                    HFlexOp::OPCODE => Box::new(HFlexOp),
                    FlexOp::OPCODE => Box::new(FlexOp),
                    HFlex1Op::OPCODE => Box::new(HFlex1Op),
                    Flex1Op::OPCODE => Box::new(Flex1Op),
                    _ => return Err(CharStringReadError::UnexpectedTwoByteOperator(b2)),
                }
            }

            255 => {
                // 32-bit signed number in 16.16 fixed-point format (Type 2 CharString).
                // Read the 255 marker byte plus 4 payload bytes (big-endian two's complement).
                // We currently store operands as i32, so we truncate the fractional part.
                let b1 = cur.read_u8()?;
                let b2 = cur.read_u8()?;
                let b3 = cur.read_u8()?;
                let b4 = cur.read_u8()?;
                let raw = i32::from_be_bytes([b1, b2, b3, b4]);
                // Truncate 16.16 fixed to integer.
                let v = raw >> 16;
                Box::new(NumberOp { value: v })
            }
            _ => {
                return Err(CharStringReadError::UnexpectedOperator(b0));
            }
        };
        ops.push(op);
    }
    Ok(ops)
}
