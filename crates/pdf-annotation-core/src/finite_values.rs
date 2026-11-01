//! Allocation-free walk over a serializable value that rejects nonfinite numbers,
//! which JSON cannot preserve, and nesting deeper than [`MAX_DEPTH`] containers.
use crate::error::ValidationError;
use serde::ser::{self, Serialize};

/// Deepest container nesting accepted, bounding recursion in later serialization.
const MAX_DEPTH: usize = 64;

/// Check every number in `value` is finite and its nesting stays within [`MAX_DEPTH`].
///
/// Returns `NonFinite` or `TooDeep`, or `InvalidMetadata` when a `Serialize` impl
/// fails; does not allocate or panic.
pub(crate) fn check(value: &impl Serialize) -> Result<(), ValidationError> {
    value
        .serialize(Walk { depth: 0 })
        .map_err(|error| match error {
            WalkError::NonFinite => ValidationError::NonFinite,
            WalkError::TooDeep => ValidationError::TooDeep,
            WalkError::Custom => ValidationError::InvalidMetadata {
                field: "unserializable value",
            },
        })
}

/// Walk failure; `Custom` reports a `Serialize` impl's own error, which none here raise.
#[derive(Debug)]
enum WalkError {
    NonFinite,
    TooDeep,
    Custom,
}

impl std::fmt::Display for WalkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NonFinite => "nonfinite number",
            Self::TooDeep => "nesting too deep",
            Self::Custom => "serialization failed",
        })
    }
}

impl std::error::Error for WalkError {}

impl ser::Error for WalkError {
    fn custom<T: std::fmt::Display>(_message: T) -> Self {
        Self::Custom
    }
}

#[derive(Clone, Copy)]
struct Walk {
    depth: usize,
}

impl Walk {
    fn child(self) -> Result<Self, WalkError> {
        if self.depth >= MAX_DEPTH {
            return Err(WalkError::TooDeep);
        }
        Ok(Self {
            depth: self.depth.saturating_add(1),
        })
    }

    fn float(value: f64) -> Result<(), WalkError> {
        if value.is_finite() {
            Ok(())
        } else {
            Err(WalkError::NonFinite)
        }
    }
}

/// Scalar serializer methods that accept every value.
macro_rules! accept {
    ($($method:ident($($ty:ty),*);)*) => {
        $(fn $method(self, $(_: $ty),*) -> Result<(), WalkError> {
            Ok(())
        })*
    };
}

impl ser::Serializer for Walk {
    type Ok = ();
    type Error = WalkError;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    accept! {
        serialize_bool(bool);
        serialize_i8(i8);
        serialize_i16(i16);
        serialize_i32(i32);
        serialize_i64(i64);
        serialize_i128(i128);
        serialize_u8(u8);
        serialize_u16(u16);
        serialize_u32(u32);
        serialize_u64(u64);
        serialize_u128(u128);
        serialize_char(char);
        serialize_str(&str);
        serialize_bytes(&[u8]);
        serialize_none();
        serialize_unit();
        serialize_unit_struct(&'static str);
        serialize_unit_variant(&'static str, u32, &'static str);
    }

    fn serialize_f32(self, value: f32) -> Result<(), WalkError> {
        Self::float(f64::from(value))
    }
    fn serialize_f64(self, value: f64) -> Result<(), WalkError> {
        Self::float(value)
    }
    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<(), WalkError> {
        value.serialize(self.child()?)
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<(), WalkError> {
        value.serialize(self.child()?)
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        value: &T,
    ) -> Result<(), WalkError> {
        value.serialize(self.child()?)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self, WalkError> {
        self.child()
    }
    fn serialize_tuple(self, _: usize) -> Result<Self, WalkError> {
        self.child()
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Self, WalkError> {
        self.child()
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self, WalkError> {
        self.child()
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self, WalkError> {
        self.child()
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self, WalkError> {
        self.child()
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self, WalkError> {
        self.child()
    }
}

/// Compound serializers that walk each element, field, key, and value at this depth.
macro_rules! walk_compound {
    ($($trait:ident { $($method:ident($($key:ty)?);)* })*) => {
        $(impl ser::$trait for Walk {
            type Ok = ();
            type Error = WalkError;
            $(fn $method<T: ?Sized + Serialize>(
                &mut self,
                $(_: $key,)?
                value: &T,
            ) -> Result<(), WalkError> {
                value.serialize(*self)
            })*
            fn end(self) -> Result<(), WalkError> {
                Ok(())
            }
        })*
    };
}

walk_compound! {
    SerializeSeq { serialize_element(); }
    SerializeTuple { serialize_element(); }
    SerializeTupleStruct { serialize_field(); }
    SerializeTupleVariant { serialize_field(); }
    SerializeStruct { serialize_field(&'static str); }
    SerializeStructVariant { serialize_field(&'static str); }
    SerializeMap { serialize_key(); serialize_value(); }
}
