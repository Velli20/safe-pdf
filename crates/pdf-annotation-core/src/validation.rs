//! Allocation-free traversal of retained metadata before it enters live state.
use crate::error::ValidationError;
use serde::ser::{self, Serialize};

impl ser::Error for ValidationError {
    fn custom<T: std::fmt::Display>(_message: T) -> Self {
        Self::InvalidMetadata {
            field: "serialized metadata",
        }
    }
}

pub(crate) fn validate(value: &impl Serialize) -> Result<(), ValidationError> {
    value.serialize(Check { depth: 0 })
}

#[derive(Clone, Copy)]
struct Check {
    depth: usize,
}
impl Check {
    fn child(self) -> Result<Self, ValidationError> {
        if self.depth >= 64 {
            return Err(ValidationError::InvalidMetadata {
                field: "metadata nesting exceeds 64 containers",
            });
        }
        Ok(Self {
            depth: self.depth.saturating_add(1),
        })
    }
}

impl ser::Serializer for Check {
    type Ok = ();
    type Error = ValidationError;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;
    fn serialize_bool(self, _value: bool) -> Result<(), ValidationError> {
        Ok(())
    }
    fn serialize_i8(self, _value: i8) -> Result<(), ValidationError> {
        Ok(())
    }
    fn serialize_i16(self, _value: i16) -> Result<(), ValidationError> {
        Ok(())
    }
    fn serialize_i32(self, _value: i32) -> Result<(), ValidationError> {
        Ok(())
    }
    fn serialize_i64(self, _value: i64) -> Result<(), ValidationError> {
        Ok(())
    }
    fn serialize_i128(self, _value: i128) -> Result<(), ValidationError> {
        Ok(())
    }
    fn serialize_u8(self, _value: u8) -> Result<(), ValidationError> {
        Ok(())
    }
    fn serialize_u16(self, _value: u16) -> Result<(), ValidationError> {
        Ok(())
    }
    fn serialize_u32(self, _value: u32) -> Result<(), ValidationError> {
        Ok(())
    }
    fn serialize_u64(self, _value: u64) -> Result<(), ValidationError> {
        Ok(())
    }
    fn serialize_u128(self, _value: u128) -> Result<(), ValidationError> {
        Ok(())
    }
    fn serialize_char(self, _value: char) -> Result<(), ValidationError> {
        Ok(())
    }
    fn serialize_str(self, _value: &str) -> Result<(), ValidationError> {
        Ok(())
    }
    fn serialize_bytes(self, _value: &[u8]) -> Result<(), ValidationError> {
        Ok(())
    }
    fn serialize_f32(self, value: f32) -> Result<(), ValidationError> {
        if value.is_finite() {
            Ok(())
        } else {
            Err(ValidationError::NonFiniteMetadata)
        }
    }
    fn serialize_f64(self, value: f64) -> Result<(), ValidationError> {
        if value.is_finite() {
            Ok(())
        } else {
            Err(ValidationError::NonFiniteMetadata)
        }
    }
    fn serialize_none(self) -> Result<(), ValidationError> {
        Ok(())
    }
    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<(), ValidationError> {
        value.serialize(self.child()?)
    }
    fn serialize_unit(self) -> Result<(), ValidationError> {
        Ok(())
    }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<(), ValidationError> {
        Ok(())
    }
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
    ) -> Result<(), ValidationError> {
        Ok(())
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<(), ValidationError> {
        value.serialize(self.child()?)
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        value: &T,
    ) -> Result<(), ValidationError> {
        value.serialize(self.child()?)
    }
    fn serialize_seq(self, _len: Option<usize>) -> Result<Self, ValidationError> {
        self.child()
    }
    fn serialize_tuple(self, _len: usize) -> Result<Self, ValidationError> {
        self.child()
    }
    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self, ValidationError> {
        self.child()
    }
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self, ValidationError> {
        self.child()
    }
    fn serialize_map(self, _len: Option<usize>) -> Result<Self, ValidationError> {
        self.child()
    }
    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self, ValidationError> {
        self.child()
    }
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self, ValidationError> {
        self.child()
    }
}
impl ser::SerializeSeq for Check {
    type Ok = ();
    type Error = ValidationError;
    fn serialize_element<T: ?Sized + Serialize>(
        &mut self,
        value: &T,
    ) -> Result<(), ValidationError> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), ValidationError> {
        Ok(())
    }
}
impl ser::SerializeTuple for Check {
    type Ok = ();
    type Error = ValidationError;
    fn serialize_element<T: ?Sized + Serialize>(
        &mut self,
        value: &T,
    ) -> Result<(), ValidationError> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), ValidationError> {
        Ok(())
    }
}
impl ser::SerializeTupleStruct for Check {
    type Ok = ();
    type Error = ValidationError;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), ValidationError> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), ValidationError> {
        Ok(())
    }
}
impl ser::SerializeTupleVariant for Check {
    type Ok = ();
    type Error = ValidationError;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), ValidationError> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), ValidationError> {
        Ok(())
    }
}
impl ser::SerializeStruct for Check {
    type Ok = ();
    type Error = ValidationError;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        _key: &'static str,
        value: &T,
    ) -> Result<(), ValidationError> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), ValidationError> {
        Ok(())
    }
}
impl ser::SerializeStructVariant for Check {
    type Ok = ();
    type Error = ValidationError;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        _key: &'static str,
        value: &T,
    ) -> Result<(), ValidationError> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), ValidationError> {
        Ok(())
    }
}
impl ser::SerializeMap for Check {
    type Ok = ();
    type Error = ValidationError;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), ValidationError> {
        value.serialize(*self)
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), ValidationError> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), ValidationError> {
        Ok(())
    }
}
