//! The Colour Specification boxes of a JP2 header or a JPX colour group.
//!
//! Part 1 I.5.3.3 requires a JP2 reader to use the first box and ignore the
//! rest. Part 2 instead lets a file offer alternatives, each ranked by a
//! precedence and an approximation, so that a reader can take the description
//! it is able to honour. The ranking rule is left to the caller here: this
//! decoder applies no colour transform, a PDF `/ColorSpace` entry may override
//! the file's description entirely, and the whole list stays available. The
//! default is the first description this crate can interpret, which is the
//! first box whenever a file names a colour space the crate models.

use crate::{
    Jpeg2000Error,
    box_reader::{BoxKind, Jp2Box, Jp2BoxReader},
    color_space::EnumeratedColorSpace,
    container::{ColorSpecification, InputFormat},
    jp2::ContainerError,
};

/// Bytes before the variable part of a Colour Specification box.
const COLOR_PREFIX_BYTES: usize = 3;
/// Enumerated colour-space method.
const ENUMERATED_COLOR: u8 = 1;
/// Restricted ICC colour-space method.
const RESTRICTED_ICC: u8 = 2;
/// General ICC colour-space method permitted by JPX.
const GENERAL_ICC: u8 = 3;

/// One colour description with the fields that rank it among alternatives.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorDescription<'a> {
    specification: ColorSpecification<'a>,
    precedence: u8,
    approximation: u8,
}

impl<'a> ColorDescription<'a> {
    /// Borrows an enumerated colour value or an ICC profile payload.
    ///
    /// A general ICC profile is accepted only in a JPX container; JP2 permits
    /// the restricted profile form alone.
    fn parse(box_view: Jp2Box<'a>, format: InputFormat) -> Result<Self, ContainerError> {
        let Some((prefix, body)) = box_view.payload().split_at_checked(COLOR_PREFIX_BYTES) else {
            return Err(box_view.into());
        };
        let [method, precedence, approximation] = *prefix
            .first_chunk::<COLOR_PREFIX_BYTES>()
            .ok_or_else(|| ContainerError::from(box_view))?;
        let specification = match (method, body) {
            (ENUMERATED_COLOR, [a, b, c, d]) => ColorSpecification::Enumerated {
                code: u32::from_be_bytes([*a, *b, *c, *d]),
            },
            (RESTRICTED_ICC, profile) if !profile.is_empty() => ColorSpecification::Icc { profile },
            (GENERAL_ICC, profile) if !profile.is_empty() && format == InputFormat::Jpx => {
                ColorSpecification::Icc { profile }
            }
            _ => return Err(box_view.into()),
        };
        Ok(Self {
            specification,
            precedence,
            approximation,
        })
    }

    /// Returns the colour description itself.
    pub fn specification(&self) -> ColorSpecification<'a> {
        self.specification
    }

    /// Returns the `PREC` field ranking this description among alternatives.
    pub fn precedence(&self) -> u8 {
        self.precedence
    }

    /// Returns the `APPROX` field stating how closely the description fits.
    pub fn approximation(&self) -> u8 {
        self.approximation
    }

    /// Returns whether this crate can name what the description means.
    ///
    /// An ICC profile is interpretable in the sense that a renderer receives
    /// the profile bytes; an enumeration outside Annex I and Annex M is not,
    /// because nothing downstream can act on the code.
    fn is_interpretable(&self) -> bool {
        match self.specification {
            ColorSpecification::Enumerated { code } => EnumeratedColorSpace::try_from(code).is_ok(),
            ColorSpecification::Icc { .. } => true,
        }
    }
}

/// The Colour Specification boxes of one superbox, read on demand.
///
/// The list is a view of the enclosing box, so it copies nothing and each
/// description is parsed only when a caller asks for it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorSpecifications<'a> {
    parent: Jp2Box<'a>,
    format: InputFormat,
}

impl<'a> ColorSpecifications<'a> {
    /// Views the Colour Specification children of one superbox.
    pub(crate) fn new(parent: Jp2Box<'a>, format: InputFormat) -> Self {
        Self { parent, format }
    }

    /// Returns each colour description in file order.
    pub fn iter(&self) -> impl Iterator<Item = Result<ColorDescription<'a>, Jpeg2000Error>> {
        let mut children = self.parent.children();
        let format = self.format;
        let mut finished = false;
        core::iter::from_fn(move || {
            if finished {
                return None;
            }
            match Self::next_color(&mut children) {
                Ok(Some(child)) => Some(ColorDescription::parse(child, format).map_err(Into::into)),
                Ok(None) => {
                    finished = true;
                    None
                }
                Err(error) => {
                    finished = true;
                    Some(Err(error))
                }
            }
        })
    }

    /// Returns the description this crate uses when the caller states none.
    ///
    /// # Errors
    ///
    /// Returns a container error when a Colour Specification box is malformed.
    pub fn primary(&self) -> Result<Option<ColorDescription<'a>>, Jpeg2000Error> {
        let mut first = None;
        for description in self.iter() {
            let description = description?;
            if description.is_interpretable() {
                return Ok(Some(description));
            }
            first = first.or(Some(description));
        }
        Ok(first)
    }

    /// Advances to the next Colour Specification child of the superbox.
    fn next_color(children: &mut Jp2BoxReader<'a>) -> Result<Option<Jp2Box<'a>>, Jpeg2000Error> {
        while let Some(child) = children.next_box()? {
            if child.kind() == BoxKind::ColorSpecification {
                return Ok(Some(child));
            }
        }
        Ok(None)
    }
}
