use std::mem;

use crate::{calculator::CalcError, operator::Operator};

/// PDF whitespace bytes, which separate tokens.
const WHITESPACE: &[u8] = b"\0\t\n\x0C\r ";

/// PostScript delimiter bytes, which end a word.
const DELIMITERS: &[u8] = b"()<>[]{}/%";

/// One lexical token of a Type 4 program.
enum Token<'a> {
    /// `{`, the start of a procedure.
    ProcBegin,
    /// `}`, the end of a procedure.
    ProcEnd,
    /// An operator name or a literal.
    Word(&'a [u8]),
}

/// Splits Type 4 program bytes into tokens, skipping whitespace and `%` comments.
struct Lexer<'a> {
    bytes: &'a [u8],
}

impl<'a> Lexer<'a> {
    /// Consumes and returns the leading bytes that satisfy `matches`.
    fn take_while(&mut self, matches: impl Fn(&u8) -> bool) -> &'a [u8] {
        let len = self
            .bytes
            .iter()
            .position(|byte| !matches(byte))
            .unwrap_or(self.bytes.len());
        let (taken, rest) = self
            .bytes
            .split_at_checked(len)
            .unwrap_or((self.bytes, &[]));
        self.bytes = rest;
        taken
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Result<Token<'a>, CalcError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.take_while(|byte| WHITESPACE.contains(byte));
            let (&first, rest) = self.bytes.split_first()?;
            let token = match first {
                // A comment runs to the end of the line.
                b'%' => {
                    self.take_while(|byte| !matches!(byte, b'\r' | b'\n'));
                    continue;
                }
                b'{' => Ok(Token::ProcBegin),
                b'}' => Ok(Token::ProcEnd),
                _ if DELIMITERS.contains(&first) => Err(CalcError::UnexpectedDelimiter(first)),
                _ => {
                    let word = self.take_while(|byte| {
                        !WHITESPACE.contains(byte) && !DELIMITERS.contains(byte)
                    });
                    return Some(Ok(Token::Word(word)));
                }
            };
            self.bytes = rest;
            return Some(token);
        }
    }
}

/// A procedure being read, with the finished procedures that wait for `if` or `ifelse`.
#[derive(Default)]
struct Procedure {
    ops: Vec<Operator>,
    operands: Vec<Vec<Operator>>,
}

impl Procedure {
    /// Appends the operator a word names. `if` and `ifelse` take the pending
    /// procedures as operands; any other word must not follow a procedure.
    fn push_word(&mut self, word: &[u8]) -> Result<(), CalcError> {
        let op = match word {
            b"if" => {
                let [then]: [Vec<Operator>; 1] = mem::take(&mut self.operands)
                    .try_into()
                    .map_err(|_| CalcError::MissingIfBlock)?;
                Operator::If(then)
            }
            b"ifelse" => {
                let [then, otherwise]: [Vec<Operator>; 2] = mem::take(&mut self.operands)
                    .try_into()
                    .map_err(|_| CalcError::MissingIfElseBlocks)?;
                Operator::IfElse(then, otherwise)
            }
            word if self.operands.is_empty() => Operator::try_from(word)?,
            _ => return Err(CalcError::UnexpectedProcedure),
        };
        self.ops.push(op);
        Ok(())
    }

    /// Returns the operators of a procedure whose closing `}` was reached.
    fn finish(self) -> Result<Vec<Operator>, CalcError> {
        if self.operands.is_empty() {
            Ok(self.ops)
        } else {
            Err(CalcError::UnexpectedProcedure)
        }
    }
}

/// Parses a Type 4 function program into the operators of its outer procedure.
///
/// The program is one `{ ... }` procedure, optionally surrounded by whitespace
/// and `%` comments. Nested procedures are only allowed as the operands of
/// `if` and `ifelse`, which store them as `Operator::If` / `Operator::IfElse`.
/// Nesting is tracked with an explicit stack, so deep nesting cannot overflow
/// the native stack.
pub fn parse(program: &[u8]) -> Result<Vec<Operator>, CalcError> {
    let mut tokens = Lexer { bytes: program };
    if !matches!(tokens.next().transpose()?, Some(Token::ProcBegin)) {
        return Err(CalcError::MissingProgramProcedure);
    }

    let mut procedures = vec![Procedure::default()];
    while let Some(token) = tokens.next().transpose()? {
        match token {
            Token::ProcBegin => procedures.push(Procedure::default()),
            Token::Word(word) => procedures
                .last_mut()
                .ok_or(CalcError::UnbalancedBraces)?
                .push_word(word)?,
            Token::ProcEnd => {
                let finished = procedures
                    .pop()
                    .ok_or(CalcError::UnbalancedBraces)?
                    .finish()?;
                match procedures.last_mut() {
                    Some(parent) => parent.operands.push(finished),
                    // The outer procedure closed; only whitespace and comments may follow.
                    None if tokens.next().is_none() => return Ok(finished),
                    None => return Err(CalcError::UnbalancedBraces),
                }
            }
        }
    }

    Err(CalcError::UnbalancedBraces)
}
