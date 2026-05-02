use serde::{
    Deserialize,
    de::{self, Visitor},
};

use crate::{Error, FieldType, Value, error::UnexpectedType};

pub struct ValueDeserializer {
    value: Value,
}

impl ValueDeserializer {
    #[inline]
    pub fn new(value: Value) -> Self {
        Self { value }
    }
}

macro_rules! forward_unimplemented {
    ($($ty:ty),*) => {
        paste::paste! {
            fn [<deserialize_ $ty>]<V>(self, visitor: V) -> Result<V::Value, Error>
        }
    };
}

macro_rules! impl_deser {
    ($($nbt_ty:ident as $ty:ty),*) => {
        paste::paste! {
            $(
                fn [<deserialize_ $ty>]<V>(self, visitor: V) -> Result<V::Value, Error> where V: Visitor<'de> {
                    match self.value {
                        Value::$nbt_ty(v) => visitor.[<visit_ $ty>](v),
                        _ => Err(Error::UnexpectedType(UnexpectedType {
                            expected: FieldType::$nbt_ty,
                            actual: self.value.as_ty()
                        }))
                    }
                }
            )*
        }
    }
}

impl<'de> de::Deserializer<'de> for ValueDeserializer {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            Value::Byte(v) => visitor.visit_i8(v),
            Value::Short(v) => visitor.visit_i16(v),
            Value::Int(v) => visitor.visit_i32(v),
            Value::Long(v) => visitor.visit_i64(v),
            Value::Float(v) => visitor.visit_f32(v),
            Value::Double(v) => visitor.visit_f64(v),
            Value::ByteArray(v) => visitor.visit_byte_buf(v),
            Value::String(v) => visitor.visit_string(v),
            Value::List(v) => visitor.visit_seq(v),
            Value::Compound(v) => visitor.visit_map(v),
            Value::IntArray(v) => visitor.visit_seq(v),
            Value::LongArray(v) => visitor.visit_seq(v),
        }
    }

    impl_deser!(
        Byte as i8,
        Short as i16,
        Int as i32,
        Long as i64,
        Float as f32,
        Double as f64
    );

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            Value::Byte(v) => visitor.visit_bool(v != 0),
            _ => Err(Error::UnexpectedType(UnexpectedType {
                expected: FieldType::Byte,
                actual: self.value.as_ty(),
            })),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            Value::String(v) => visitor.visit_str(&v),
            _ => Err(Error::UnexpectedType(UnexpectedType {
                expected: FieldType::String,
                actual: self.value.as_ty(),
            })),
        }
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            Value::String(v) => visitor.visit_string(v),
            _ => Err(Error::UnexpectedType(UnexpectedType {
                expected: FieldType::String,
                actual: self.value.as_ty(),
            })),
        }
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            Value::ByteArray(v) => visitor.visit_bytes(&v),
            _ => Err(Error::UnexpectedType(UnexpectedType {
                expected: FieldType::ByteArray,
                actual: self.value.as_ty(),
            })),
        }
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            Value::ByteArray(v) => visitor.visit_byte_buf(v),
            _ => Err(Error::UnexpectedType(UnexpectedType {
                expected: FieldType::ByteArray,
                actual: self.value.as_ty(),
            })),
        }
    }
}

pub fn from_value<'de, T: Deserialize<'de>>(value: Value) -> Result<T, Error> {
    let deserializer = ValueDeserializer::new(value);
    T::deserialize(deserializer)
}
