use std::fmt;
use std::ops::Deref;

use crate::{OperandContext, error::PdfOperatorError};
use pdf_object_reader::{ReadResult, pdf_name::PdfName};

/// The most color components stored without a heap allocation.
///
/// With the length byte and the enum tag this keeps the value the size of a `Vec`.
const INLINE_CAPACITY: usize = 5;

/// The storage of [`ColorComponents`]: few components live inline, more go to the heap.
#[derive(Clone)]
enum Storage {
    Inline {
        len: u8,
        values: [f32; INLINE_CAPACITY],
    },
    Heap(Box<[f32]>),
}

/// The color component values of a color-setting operator.
///
/// Up to five components, which covers every device and most special color
/// spaces, are stored inline without a heap allocation.
#[derive(Clone)]
pub struct ColorComponents(Storage);

impl ColorComponents {
    /// Reads the first `count` operands as color components.
    #[inline]
    fn read(operands: &mut OperandContext<'_, '_>, count: usize) -> ReadResult<Self> {
        let mut values = [0.0; INLINE_CAPACITY];
        match (u8::try_from(count), values.get_mut(..count)) {
            (Ok(len), Some(prefix)) => {
                for (index, value) in prefix.iter_mut().enumerate() {
                    *value = operands.at(index)?;
                }
                Ok(Self(Storage::Inline { len, values }))
            }
            _ => (0..count)
                .map(|index| operands.at(index))
                .collect::<ReadResult<Box<[f32]>>>()
                .map(|values| Self(Storage::Heap(values))),
        }
    }
}

impl Deref for ColorComponents {
    type Target = [f32];

    fn deref(&self) -> &[f32] {
        match &self.0 {
            Storage::Inline { len, values } => values.get(..usize::from(*len)).unwrap_or(&[]),
            Storage::Heap(values) => values,
        }
    }
}

impl fmt::Debug for ColorComponents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl PartialEq for ColorComponents {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

/// The operands of a color-setting operator whose color space takes any number
/// of components (`SC`, `SCN`, `sc`, `scn`).
#[derive(Debug, Clone, PartialEq)]
pub struct ColorOperands {
    /// Color component values.
    pub components: ColorComponents,
    /// An optional name of a pattern.
    pub pattern: Option<PdfName>,
}

impl ColorOperands {
    /// Reads the operands of `SCN` or `scn`: the color components, optionally
    /// followed by a pattern name.
    #[inline]
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let len = operands.array().len();
        // The pattern name should come last, after the numeric color components
        let count = operands
            .array()
            .iter()
            .position(|object| object.is_name())
            .unwrap_or(len);

        Ok(Self {
            components: ColorComponents::read(operands, count)?,
            pattern: (count < len).then(|| operands.at(count)).transpose()?,
        })
    }

    /// Reads the operands of `operator` (`SC` or `sc`), which takes at least one
    /// color component and no pattern.
    #[inline]
    pub(crate) fn read_without_pattern(
        operands: &mut OperandContext<'_, '_>,
        operator: &'static str,
    ) -> ReadResult<Self> {
        let count = operands.array().len();

        if count == 0 {
            return Err(PdfOperatorError::OperandCountMismatch {
                operator: operator.to_string(),
                expected: 1,
                actual: 0,
            }
            .into());
        }

        Ok(Self {
            components: ColorComponents::read(operands, count)?,
            pattern: None,
        })
    }
}
