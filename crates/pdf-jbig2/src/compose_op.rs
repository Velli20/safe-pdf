#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ComposeOp {
    Or,
    And,
    Xor,
    Xnor,
    Replace,
}

impl From<u8> for ComposeOp {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Or,
            1 => Self::And,
            2 => Self::Xor,
            3 => Self::Xnor,
            _ => Self::Replace,
        }
    }
}

impl ComposeOp {
    pub(crate) fn apply(self, dst: u8, src: u8) -> u8 {
        match self {
            Self::Or => dst | src,
            Self::And => dst & src,
            Self::Xor => dst ^ src,
            Self::Xnor => (!(dst ^ src)) & 1,
            Self::Replace => src,
        }
    }

    pub(crate) fn apply_byte(self, dst: u8, src: u8) -> u8 {
        match self {
            Self::Or => dst | src,
            Self::And => dst & src,
            Self::Xor => dst ^ src,
            Self::Xnor => !(dst ^ src),
            Self::Replace => src,
        }
    }
}
