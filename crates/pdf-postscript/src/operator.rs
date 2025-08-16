/// Represents a subset of PostScript operators (and literals) supported by the PDF
/// PostScript-like calculator in this crate. Each variant corresponds to an
/// operator defined by the PostScript language reference unless otherwise noted.
///
/// Stack notation in the docs below uses the conventional PostScript style:
/// (before -- after). For example: (a b -- a+b) means the operator pops `b` then
/// `a` (with `b` being on the top of the stack) and pushes the result `a+b`.
#[derive(Debug, PartialEq, Clone)]
pub enum Operator {
    /// add (a b -- a+b)
    Add,
    /// sub (a b -- a-b)
    Sub,
    /// mul (a b -- a*b)
    Mul,
    /// div (a b -- a/b)
    Div,
    /// dup (x -- x x) duplicates the top stack element.
    Dup,
    /// exch (a b -- b a) swaps the top two elements.
    Exch,
    /// pop (x -- ) removes the top element.
    Pop,
    /// eq (a b -- bool) pushes true if a == b else false.
    Eq,
    /// ne (a b -- bool) pushes true if a != b else false.
    Ne,
    /// gt (a b -- bool) true if a > b.
    Gt,
    /// lt (a b -- bool) true if a < b.
    Lt,
    /// ge (a b -- bool) true if a >= b.
    Ge,
    /// le (a b -- bool) true if a <= b.
    Le,
    /// and (x y -- x&y) logical (if both booleans) or bitwise (if numbers) AND.
    And,
    /// or (x y -- x|y) logical (if both booleans) or bitwise (if numbers) OR.
    Or,
    /// not (x -- !x) logical (if boolean) or bitwise complement (if integer-like).
    Not,
    /// copy (x1 .. xn n -- x1 .. xn x1 .. xn) duplicates the top `n` elements.
    Copy,
    /// roll (x1 .. xn j n -- rotated) rotates the top `n` elements by `j` steps.
    Roll,
    /// sqrt (x -- sqrt(x)) square root.
    Sqrt,
    /// abs (x -- |x|) absolute value.
    Abs,
    /// cvi (x -- int) convert to integer (truncate toward zero in PostScript).
    Cvi,
    /// mod (a b -- a%b) remainder (same sign behavior as PostScript: sign of a).
    Mod,
    /// truncate (x -- truncated) implementation-specific explicit truncation.
    Truncate,
    /// if (bool proc -- ) executes `proc` iff bool is true. Here stored as
    /// already-parsed operator sequence (executable array).
    If(Vec<Operator>),
    /// ifelse (bool proc_true proc_false -- ) executes one of the procedures
    /// depending on the boolean value.
    IfElse(Vec<Operator>, Vec<Operator>),
    /// Numeric literal (integer or real). Kept as f64 for simplicity.
    Number(f64),
}
