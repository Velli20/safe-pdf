//! Extensible typed decoding with one object-bound context per invocation.

use num_traits::FromPrimitive;
use pdf_graphics::{LineCap, LineJoin, MaskMode, TextRenderingMode};
use std::any::TypeId;

use crate::context::ObjectContext;
use crate::error::{ObjectReadError, ReadResult};
use crate::handle::ObjectHandle;
use crate::inline_bytes::InlineBytes;
use crate::object_error::ObjectError;
use crate::object_kind::ObjectKind;
use crate::object_variant::ObjectVariant;
use crate::pdf_array::PdfArray;
use crate::pdf_name::PdfName;
use crate::pdf_string::PdfString;
use crate::reader::ObjectAccess;
use crate::{dictionary::Dictionary, stream::StreamObject};
use pdf_graphics::{color::Color, interval::Interval, rect::Rect, transform::Transform};

/// Decodes a resolved PDF value using one object-bound context.
///
/// Implement this trait on client-owned types. The context exposes the current
/// value, typed child reads, content-stream IDs, and cycle-protected access.
/// Implementations should use context reads for recursion rather than starting
/// fresh sessions.
///
/// Outputs are `'static`, so the reader can tell decoders apart when it detects
/// cycles (see [`FromPdfObject::cycle_key`]). Shared reading additionally requires
/// `Self: Send + Sync`.
///
/// Implementations must not retain strong pointers to other cached values as
/// graph edges: store `ObjectHandle<T>` for indirect edges to avoid ownership
/// cycles. Do not demand a pending handle's value while constructing a cycle.
///
/// Shared reading rejects a decoder whose output contains a non-thread-safe Rc:
///
/// ```compile_fail,E0277
/// use std::rc::Rc;
/// use pdf_object_reader::{
///     FromPdfObject, ObjectAccess, ObjectContext, object_id::ObjectId, ObjectReader,
///     ObjectSource, ReadResult,
/// };
///
/// struct Local(Rc<u8>);
///
/// impl FromPdfObject for Local {
///     fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self>
///     {
///         todo!("client decoder")
///     }
/// }
///
/// fn invalid_shared_read<S: ObjectSource>(reader: &ObjectReader<S>, id: ObjectId) {
///     let _ = reader.read_shared_indirect::<Local>(id);
/// }
/// ```
pub trait FromPdfObject: Sized + 'static {
    /// Converts the context's object, retaining its session for all nested reads.
    ///
    /// Return domain failures using `ObjectReadError::Decode`; shape and child
    /// failures propagate through `ReadResult`.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self>;

    /// Identifies this decoder when the reader detects cycles: an object a decoder is
    /// still decoding cannot be entered again by a decoder with the same key.
    ///
    /// A decoder that decodes its object only through another decoder returns that
    /// decoder's key, so reaching the object through either one is the same visit.
    #[inline]
    fn cycle_key() -> TypeId {
        TypeId::of::<Self>()
    }
}

macro_rules! value_decoder {
    ($type:ty, $variant:ident, $kind:ident) => {
        impl FromPdfObject for $type {
            fn from_pdf_object(
                context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
            ) -> ReadResult<Self> {
                match context.object() {
                    ObjectVariant::$variant(value) => Ok(value.clone()),
                    _ => Err(ObjectReadError::TypeMismatch {
                        expected: ObjectKind::$kind,
                        actual: context.object().kind(),
                    }),
                }
            }
        }
    };
}

value_decoder!(Dictionary, Dictionary, Dictionary);
value_decoder!(StreamObject, Stream, Stream);

impl FromPdfObject for () {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object() {
            ObjectVariant::Null => Ok(()),
            _ => Err(ObjectReadError::TypeMismatch {
                expected: ObjectKind::Null,
                actual: context.object().kind(),
            }),
        }
    }
}

impl FromPdfObject for f64 {
    #[allow(
        clippy::as_conversions,
        reason = "PDF integer-to-real conversion intentionally permits rounding"
    )]
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object() {
            ObjectVariant::Real(value) => Ok(*value),
            ObjectVariant::Integer(value) => Ok(*value as f64),
            _ => Err(ObjectReadError::TypeMismatch {
                expected: ObjectKind::Real,
                actual: context.object().kind(),
            }),
        }
    }
}

/// Decodes PDF null as absence and delegates other values to the inner decoder.
///
/// An absent dictionary key is handled by the dictionary context, not this trait.
impl<T: FromPdfObject> FromPdfObject for Option<T> {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        if context.object().kind() == ObjectKind::Null {
            Ok(None)
        } else {
            T::from_pdf_object(context).map(Some)
        }
    }

    #[inline]
    fn cycle_key() -> TypeId {
        T::cycle_key()
    }
}

/// Reads the value as a shared handle through the typed cache (see [`ObjectContext::shared`]).
///
/// A recursive edge back to a value still being decoded is a pending handle.
impl<T: FromPdfObject + Send + Sync + 'static> FromPdfObject for ObjectHandle<T> {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        context.shared()
    }
}

/// Decodes array elements in source order within the current traversal.
impl<T: FromPdfObject> FromPdfObject for Vec<T> {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        context.array()?.read_all()
    }
}

impl FromPdfObject for PdfString {
    #[inline]
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object() {
            ObjectVariant::String(value) => Ok(value.clone()),
            _ => Err(ObjectReadError::TypeMismatch {
                expected: ObjectKind::String,
                actual: context.object().kind(),
            }),
        }
    }
}

impl FromPdfObject for PdfArray {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object() {
            ObjectVariant::Array(value) => Ok(value.clone()),
            _ => Err(ObjectReadError::TypeMismatch {
                expected: ObjectKind::Array,
                actual: context.object().kind(),
            }),
        }
    }
}

macro_rules! number_decoder {
    ($($type:ty),+ $(,)?) => {
        $(
            impl FromPdfObject for $type {
                /// Decodes a resolved number with the model's checked numeric conversion.
                #[inline]
                fn from_pdf_object(
                    context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
                ) -> ReadResult<Self> {
                    // The context has already resolved references and owns their scope.
                    let value = match context.object() {
                        ObjectVariant::Integer(value) => Self::from_i64(*value),
                        ObjectVariant::Real(value) => Self::from_f64(*value),
                        other => {
                            return Err(ObjectError::TypeMismatch("Number", other.name()).into());
                        }
                    };
                    Ok(value.ok_or(ObjectError::NumberConversionError)?)
                }
            }
        )+
    };
}

number_decoder!(f32, i32, i64, u8, u16, u32, u64, usize);

macro_rules! enumerated_number_decoder {
    ($($type:ty => $expected:literal),+ $(,)?) => {
        $(
            impl FromPdfObject for $type {
                /// Decodes the number that names one value of this parameter.
                fn from_pdf_object(
                    context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
                ) -> ReadResult<Self> {
                    let found = context.decode::<i32>()?;
                    Ok(Self::from_i32(found).ok_or(ObjectError::InvalidEnumValue {
                        expected: $expected,
                        found,
                    })?)
                }
            }
        )+
    };
}

enumerated_number_decoder!(
    LineCap => "a line cap style (0 butt, 1 round, 2 projecting square)",
    LineJoin => "a line join style (0 miter, 1 round, 2 bevel)",
    TextRenderingMode => "a text rendering mode (0 to 7)",
);

impl FromPdfObject for bool {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object() {
            ObjectVariant::Boolean(value) => Ok(*value),
            _ => Err(ObjectReadError::TypeMismatch {
                expected: ObjectKind::Boolean,
                actual: context.object().kind(),
            }),
        }
    }
}

/// Reads the first N elements, validating length before decoding any child.
///
/// Some producers pad fixed-size arrays with extra entries, so trailing elements are ignored.
impl<T: FromPdfObject, const N: usize> FromPdfObject for [T; N] {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.array()?;
        if context.array().len() < N {
            return Err(crate::object_error::ObjectError::InvalidArrayLength {
                expected: N,
                found: context.array().len(),
            }
            .into());
        }
        (0..N)
            .map(|index| context.at::<T>(index))
            .collect::<ReadResult<Vec<T>>>()?
            .try_into()
            .map_err(|values: Vec<T>| {
                crate::object_error::ObjectError::InvalidArrayLength {
                    expected: N,
                    found: values.len(),
                }
                .into()
            })
    }
}

/// Reads a `[min max]` array, such as a function `/Domain`, as an interval.
///
/// Trailing elements are ignored, like the `[T; N]` decoder.
impl FromPdfObject for Interval {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let [min, max] = context.array()?.read_array::<f32, 2>()?;
        Ok(Self::new(min, max))
    }
}

/// Reads a rectangle `[x1 y1 x2 y2]` in its original coordinate order.
///
/// Trailing elements are ignored, like the `[T; N]` decoder.
impl FromPdfObject for Rect {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        Ok(Self::from(context.array()?.read_array::<f32, 4>()?))
    }
}

/// Reads an affine matrix `[a b c d e f]`.
///
/// Trailing elements are ignored, like the `[T; N]` decoder.
impl FromPdfObject for Transform {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        Ok(Self::from(context.array()?.read_array::<f32, 6>()?))
    }
}

/// Reads a device color array: one gray, three RGB, or four CMYK components.
///
/// An empty array is transparent, as for annotation colors. Nonfinite components and
/// other component counts are rejected.
impl FromPdfObject for Color {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let components: Vec<f32> = context.array()?.read_all()?;
        if components.iter().any(|component| !component.is_finite()) {
            return Err(ObjectError::NumberConversionError.into());
        }
        if components.is_empty() {
            return Ok(Self::TRANSPARENT);
        }
        Self::from_device_components(&components).ok_or_else(|| ObjectReadError::Decode {
            target: "device color",
            source: format!(
                "expected one, three, or four color components, found {}",
                components.len()
            )
            .into(),
        })
    }
}

/// Reads a soft mask's `/S` subtype. Unrecognized names are kept as `Unknown`.
impl FromPdfObject for MaskMode {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let name = context.decode::<PdfName>()?;
        Ok(match name.as_bytes() {
            b"Alpha" => Self::Alpha,
            b"Luminosity" => Self::Luminosity,
            other => Self::Unknown(Vec::from(other)),
        })
    }
}

/// Reads a name, accepting a string in its place as producers substitute one for the
/// other. Short names are copied inline; longer ones share the object's bytes.
impl FromPdfObject for PdfName {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object() {
            ObjectVariant::String(value) => Ok(Self::from(value.bytes.clone())),
            other => Err(ObjectError::TypeMismatch("Name", other.name()).into()),
        }
    }
}

/// Reads the bytes of a name or string, accepting producer substitutions between them.
/// Short values are copied inline; longer ones share the object's bytes.
impl FromPdfObject for InlineBytes {
    #[inline]
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object() {
            ObjectVariant::String(value) => Ok(value.bytes.clone()),
            other => Err(ObjectError::TypeMismatch("Bytes", other.name()).into()),
        }
    }
}

/// Reads the bytes of a name or string, accepting producer substitutions between them,
/// or the raw data of a stream.
///
/// Use `PdfString` when the PDF object kind must be enforced.
impl FromPdfObject for std::sync::Arc<[u8]> {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object() {
            ObjectVariant::String(value) => Ok(value.bytes.to_shared()),
            ObjectVariant::Stream(stream) => Ok(Self::from(stream.raw_data())),
            other => Err(ObjectError::TypeMismatch("Bytes", other.name()).into()),
        }
    }
}
