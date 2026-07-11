use std::collections::BTreeMap;
use std::fmt;
use std::hash::{Hash, Hasher};

use bstr::{BString, ByteSlice};
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::nbt::ser::RAW_STRING_TOKEN;

/// General NBT value type that can represent any value.
///
/// In case the structure of some piece of NBT data is not known, this
/// type can be used to deserialise it.
#[derive(Debug, Clone)]
pub enum Value {
    /// A signed byte.
    Byte(i8),
    /// A signed short.
    Short(i16),
    /// A signed int.
    Int(i32),
    /// A signed long.
    Long(i64),
    /// A signed float
    Float(f32),
    /// A signed double
    Double(f64),
    /// A byte array.
    ///
    /// Deserialising an NBT `ByteArray` tag into a [`Value`] yields this
    /// variant, and it re-serialises byte-for-byte as a `ByteArray` tag for all
    /// three endianness variants. When defining your own types, use
    /// [`NbtByteArray`](crate::NbtByteArray) or a
    /// [`serde_bytes`](https://crates.io/crates/serde_bytes)-style field to map
    /// to the `ByteArray` tag; a plain `Vec<i8>` serialises as a `List` of
    /// `Byte`s instead.
    ByteArray(Vec<u8>),
    /// A string of raw bytes.
    ///
    /// NBT strings are not guaranteed to be valid UTF-8 (Java uses MUTF-8 and
    /// Bedrock allows arbitrary bytes), so the raw bytes are stored in a
    /// [`bstr::BString`] rather than a [`String`]. Construction from string
    /// literals still works ergonomically, e.g. `Value::String("abc".into())`.
    String(BString),
    /// List of an arbitrary NBT value.
    List(Vec<Value>),
    /// Key-value map.
    ///
    /// Keys are stored as [`bstr::BString`] because NBT string keys are not
    /// guaranteed to be valid UTF-8. A [`BTreeMap`] is used so that the entries
    /// have a deterministic ordering.
    Compound(BTreeMap<BString, Value>),
    /// An array of integers.
    IntArray(Vec<i32>),
    /// An array of longs.
    LongArray(Vec<i64>),
}

impl Value {
    #[inline]
    #[must_use]
    pub fn discriminant(&self) -> u8 {
        match self {
            Self::Byte(_) => 1,
            Self::Short(_) => 2,
            Self::Int(_) => 3,
            Self::Long(_) => 4,
            Self::Float(_) => 5,
            Self::Double(_) => 6,
            Self::ByteArray(_) => 7,
            Self::String(_) => 8,
            Self::List(_) => 9,
            Self::Compound(_) => 10,
            Self::IntArray(_) => 11,
            Self::LongArray(_) => 12,
        }
    }
}

macro_rules! impl_access_fns {
    ($($tag: ident = $ty: ty),+) => {
        $(paste::paste! {
            #[inline]
            #[doc = concat!(
                "Returns the inner value if the tag is of a, ", stringify!($tag), ", otherwise returns self.
                This method is the same as [`as_", stringify!([<$tag:snake>]), "`](Self::as_", stringify!([<$tag:snake>]), ") but instead takes ownership of the value."
            )]
            pub fn [<into_ $tag:snake>](self) -> Result<$ty, Self> {
                match self {
                    Self::$tag(val) => Ok(val),
                    _ => Err(self)
                }
            }

            #[inline]
            #[doc = concat!(
                "Returns a reference to the inner value of the tag is the requested type is present.
                Use [`into_", stringify!([<$tag:snake>]), "`](Self::into_", stringify!([<$tag:snake>]), ")."
            )]
            pub fn [<as_ $tag:snake>](&self) -> Option<&$ty> {
                match self {
                    Self::$tag(val) => Some(val),
                    _ => None
                }
            }

            #[inline]
            #[doc = concat!(
                "Returns whether the inner value is of type `", stringify!($tag), "`."
            )]
            pub fn [<is_ $tag:snake>](&self) -> bool {
                matches!(self, Self::$tag(_))
            }
        })+
    }
}

impl Value {
    impl_access_fns!(
        Byte = i8,
        Short = i16,
        Int = i32,
        Long = i64,
        Float = f32,
        Double = f64,
        String = BString,
        List = Vec<Self>,
        Compound = BTreeMap<BString, Self>,
        ByteArray = Vec<u8>,
        IntArray = Vec<i32>,
        LongArray = Vec<i64>
    );
}

impl From<BString> for Value {
    #[inline]
    fn from(value: BString) -> Self {
        Value::String(value)
    }
}

impl From<String> for Value {
    #[inline]
    fn from(value: String) -> Self {
        Value::String(BString::from(value))
    }
}

impl From<&str> for Value {
    #[inline]
    fn from(value: &str) -> Self {
        Value::String(BString::from(value))
    }
}

impl PartialEq<Value> for Value {
    #[inline]
    fn eq(&self, rhs: &Value) -> bool {
        match self {
            Value::Byte(lhs) => rhs.as_byte() == Some(lhs),
            Value::Short(lhs) => rhs.as_short() == Some(lhs),
            Value::Int(lhs) => rhs.as_int() == Some(lhs),
            Value::Long(lhs) => rhs.as_long() == Some(lhs),
            Value::Float(lhs) => rhs.as_float() == Some(lhs),
            Value::Double(lhs) => rhs.as_double() == Some(lhs),
            Value::ByteArray(lhs) => rhs.as_byte_array().is_some_and(|rhs| lhs.as_slice() == rhs),
            Value::String(lhs) => rhs.as_string() == Some(lhs),
            Value::List(lhs) => rhs.as_list() == Some(lhs),
            Value::Compound(lhs) => rhs.as_compound() == Some(lhs),
            Value::IntArray(lhs) => rhs.as_int_array() == Some(lhs),
            Value::LongArray(lhs) => rhs.as_long_array() == Some(lhs),
        }
    }
}

impl PartialEq<i8> for Value {
    #[inline]
    fn eq(&self, rhs: &i8) -> bool {
        self.as_byte() == Some(rhs)
    }
}

impl PartialEq<i8> for &Value {
    #[inline]
    fn eq(&self, rhs: &i8) -> bool {
        self.as_byte() == Some(rhs)
    }
}

impl PartialEq<i8> for &mut Value {
    #[inline]
    fn eq(&self, rhs: &i8) -> bool {
        self.as_byte() == Some(rhs)
    }
}

impl PartialEq<i16> for Value {
    #[inline]
    fn eq(&self, rhs: &i16) -> bool {
        self.as_short() == Some(rhs)
    }
}

impl PartialEq<i16> for &Value {
    #[inline]
    fn eq(&self, rhs: &i16) -> bool {
        self.as_short() == Some(rhs)
    }
}

impl PartialEq<i16> for &mut Value {
    #[inline]
    fn eq(&self, rhs: &i16) -> bool {
        self.as_short() == Some(rhs)
    }
}

impl PartialEq<i32> for Value {
    #[inline]
    fn eq(&self, rhs: &i32) -> bool {
        self.as_int() == Some(rhs)
    }
}

impl PartialEq<i32> for &Value {
    #[inline]
    fn eq(&self, rhs: &i32) -> bool {
        self.as_int() == Some(rhs)
    }
}

impl PartialEq<i32> for &mut Value {
    #[inline]
    fn eq(&self, rhs: &i32) -> bool {
        self.as_int() == Some(rhs)
    }
}

impl PartialEq<i64> for Value {
    #[inline]
    fn eq(&self, rhs: &i64) -> bool {
        self.as_long() == Some(rhs)
    }
}

impl PartialEq<i64> for &Value {
    #[inline]
    fn eq(&self, rhs: &i64) -> bool {
        self.as_long() == Some(rhs)
    }
}

impl PartialEq<i64> for &mut Value {
    #[inline]
    fn eq(&self, rhs: &i64) -> bool {
        self.as_long() == Some(rhs)
    }
}

impl PartialEq<f32> for Value {
    #[inline]
    fn eq(&self, rhs: &f32) -> bool {
        self.as_float() == Some(rhs)
    }
}

impl PartialEq<f32> for &Value {
    #[inline]
    fn eq(&self, rhs: &f32) -> bool {
        self.as_float() == Some(rhs)
    }
}

impl PartialEq<f32> for &mut Value {
    #[inline]
    fn eq(&self, rhs: &f32) -> bool {
        self.as_float() == Some(rhs)
    }
}

impl PartialEq<f64> for Value {
    #[inline]
    fn eq(&self, rhs: &f64) -> bool {
        self.as_double() == Some(rhs)
    }
}

impl PartialEq<f64> for &Value {
    #[inline]
    fn eq(&self, rhs: &f64) -> bool {
        self.as_double() == Some(rhs)
    }
}

impl PartialEq<f64> for &mut Value {
    #[inline]
    fn eq(&self, rhs: &f64) -> bool {
        self.as_double() == Some(rhs)
    }
}

impl PartialEq<&[u8]> for Value {
    #[inline]
    fn eq(&self, rhs: &&[u8]) -> bool {
        self.as_byte_array().is_some_and(|lhs| lhs == rhs)
    }
}

impl PartialEq<&[u8]> for &Value {
    #[inline]
    fn eq(&self, rhs: &&[u8]) -> bool {
        self.as_byte_array().is_some_and(|lhs| lhs == rhs)
    }
}

impl PartialEq<&[u8]> for &mut Value {
    #[inline]
    fn eq(&self, rhs: &&[u8]) -> bool {
        self.as_byte_array().is_some_and(|lhs| lhs == rhs)
    }
}

impl PartialEq<&str> for Value {
    #[inline]
    fn eq(&self, rhs: &&str) -> bool {
        self.as_string()
            .is_some_and(|lhs| lhs.as_slice() == rhs.as_bytes())
    }
}

impl PartialEq<&str> for &Value {
    #[inline]
    fn eq(&self, rhs: &&str) -> bool {
        self.as_string()
            .is_some_and(|lhs| lhs.as_slice() == rhs.as_bytes())
    }
}

impl PartialEq<&str> for &mut Value {
    #[inline]
    fn eq(&self, rhs: &&str) -> bool {
        self.as_string()
            .is_some_and(|lhs| lhs.as_slice() == rhs.as_bytes())
    }
}

impl PartialEq<&[Value]> for Value {
    #[inline]
    fn eq(&self, rhs: &&[Value]) -> bool {
        self.as_list().is_some_and(|lhs| lhs == rhs)
    }
}

impl PartialEq<&[Value]> for &Value {
    #[inline]
    fn eq(&self, rhs: &&[Value]) -> bool {
        self.as_list().is_some_and(|lhs| lhs == rhs)
    }
}

impl PartialEq<&[Value]> for &mut Value {
    #[inline]
    fn eq(&self, rhs: &&[Value]) -> bool {
        self.as_list().is_some_and(|lhs| lhs == rhs)
    }
}

impl PartialEq<BTreeMap<BString, Value>> for Value {
    #[inline]
    fn eq(&self, rhs: &BTreeMap<BString, Value>) -> bool {
        self.as_compound() == Some(rhs)
    }
}

impl PartialEq<BTreeMap<BString, Value>> for &Value {
    #[inline]
    fn eq(&self, rhs: &BTreeMap<BString, Value>) -> bool {
        self.as_compound() == Some(rhs)
    }
}

impl PartialEq<BTreeMap<BString, Value>> for &mut Value {
    #[inline]
    fn eq(&self, rhs: &BTreeMap<BString, Value>) -> bool {
        self.as_compound() == Some(rhs)
    }
}

impl PartialEq<&[i32]> for Value {
    #[inline]
    fn eq(&self, rhs: &&[i32]) -> bool {
        self.as_int_array().is_some_and(|lhs| lhs == rhs)
    }
}

impl PartialEq<&[i32]> for &Value {
    #[inline]
    fn eq(&self, rhs: &&[i32]) -> bool {
        self.as_int_array().is_some_and(|lhs| lhs == rhs)
    }
}

impl PartialEq<&[i32]> for &mut Value {
    #[inline]
    fn eq(&self, rhs: &&[i32]) -> bool {
        self.as_int_array().is_some_and(|lhs| lhs == rhs)
    }
}

impl PartialEq<&[i64]> for Value {
    #[inline]
    fn eq(&self, rhs: &&[i64]) -> bool {
        self.as_long_array().is_some_and(|lhs| lhs == rhs)
    }
}

impl PartialEq<&[i64]> for &Value {
    #[inline]
    fn eq(&self, rhs: &&[i64]) -> bool {
        self.as_long_array().is_some_and(|lhs| lhs == rhs)
    }
}

impl PartialEq<&[i64]> for &mut Value {
    #[inline]
    fn eq(&self, rhs: &&[i64]) -> bool {
        self.as_long_array().is_some_and(|lhs| lhs == rhs)
    }
}

impl Hash for Value {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        match self {
            Value::Byte(v) => state.write_i8(*v),
            Value::Short(v) => state.write_i16(*v),
            Value::Int(v) => state.write_i32(*v),
            Value::Long(v) => state.write_i64(*v),
            Value::String(v) => state.write(v.as_slice()),
            Value::Float(v) => {
                // f32 does not implement Hash, so simply hash the byte representation
                // IEEE floats have + 0 and - 0, which are the same value but have different byte representations
                let bytes = if *v == (-0_f32) {
                    0_f32.to_le_bytes()
                } else {
                    v.to_le_bytes()
                };
                state.write(&bytes);
            }
            Value::Double(v) => {
                // f64 does not implement Hash, so simply hash the byte representation
                // IEEE floats have + 0 and - 0, which are the same value but have different byte representations
                let bytes = if *v == (-0_f64) {
                    0_f64.to_le_bytes()
                } else {
                    v.to_le_bytes()
                };
                state.write(&bytes);
            }
            Value::Compound(map) => {
                for (k, v) in map {
                    state.write(k.as_slice());
                    v.hash(state);
                }
            }
            Value::List(v) => Self::hash_slice(v, state),
            Value::ByteArray(v) => u8::hash_slice(v, state),
            Value::IntArray(v) => i32::hash_slice(v, state),
            Value::LongArray(v) => i64::hash_slice(v, state),
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(ValueVisitor)
    }
}

#[inline]
fn serialize_seq<T, S>(ser: S, seq: &[T]) -> Result<S::Ok, S::Error>
where
    T: Serialize,
    S: Serializer,
{
    let mut seq_ser = ser.serialize_seq(Some(seq.len()))?;
    for element in seq {
        seq_ser.serialize_element(element)?;
    }
    seq_ser.end()
}

impl Serialize for Value {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Value::Byte(byte) => ser.serialize_i8(*byte),
            Value::Short(short) => ser.serialize_i16(*short),
            Value::Int(int) => ser.serialize_i32(*int),
            Value::Long(long) => ser.serialize_i64(*long),
            Value::Float(float) => ser.serialize_f32(*float),
            Value::Double(double) => ser.serialize_f64(*double),
            Value::ByteArray(array) => ser.serialize_bytes(array),
            Value::String(string) => {
                if ser.is_human_readable() {
                    // Human-readable formats (e.g. SNBT) are text, so hand over
                    // a (lossily) UTF-8 decoded string.
                    ser.serialize_str(&string.to_str_lossy())
                } else {
                    // Binary NBT: emit a raw String tag via the magic newtype
                    // token so non-UTF-8 bytes round-trip losslessly.
                    ser.serialize_newtype_struct(RAW_STRING_TOKEN, string)
                }
            }
            Value::List(seq) => serialize_seq(ser, seq),
            Value::Compound(map) => {
                let human_readable = ser.is_human_readable();
                let mut map_ser = ser.serialize_map(Some(map.len()))?;
                for (k, v) in map {
                    if human_readable {
                        map_ser.serialize_entry(&k.to_str_lossy(), v)?;
                    } else {
                        map_ser.serialize_entry(&RawString(k), v)?;
                    }
                }
                map_ser.end()
            }
            Value::IntArray(seq) => serialize_seq(ser, seq),
            Value::LongArray(seq) => serialize_seq(ser, seq),
        }
    }
}

/// Serializes a [`BString`] as a raw NBT String tag via the magic newtype
/// token, so that compound keys holding non-UTF-8 bytes round-trip losslessly
/// through the binary serializer.
struct RawString<'a>(&'a BString);

impl Serialize for RawString<'_> {
    #[inline]
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ser.serialize_newtype_struct(RAW_STRING_TOKEN, self.0)
    }
}

struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    #[inline]
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("any valid NBT value")
    }

    #[inline]
    fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Byte(v as i8))
    }

    #[inline]
    fn visit_i8<E>(self, v: i8) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Byte(v))
    }

    #[inline]
    fn visit_i16<E>(self, v: i16) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Short(v))
    }

    #[inline]
    fn visit_i32<E>(self, v: i32) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Int(v))
    }

    #[inline]
    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Long(v))
    }

    #[inline]
    fn visit_f32<E>(self, v: f32) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Float(v))
    }

    #[inline]
    fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Double(v))
    }

    #[inline]
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::String(BString::from(v)))
    }

    #[inline]
    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::String(BString::from(v)))
    }

    // NBT String tags are routed through the byte-oriented entry points so that
    // non-UTF-8 payloads reach a `Value` losslessly as a `Value::String`. NBT
    // `ByteArray` tags never reach here: they are surfaced through
    // `visit_newtype_struct` so they stay distinct from strings (see below).
    #[inline]
    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::String(BString::from(v)))
    }

    #[inline]
    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::String(BString::from(v)))
    }

    // NBT `ByteArray` tags are surfaced through a newtype struct by both the
    // binary and SNBT deserializers. String tags arrive via `visit_byte_buf`
    // and lists via `visit_seq`, so routing byte arrays here keeps all three
    // distinct and lets a `ByteArray` round-trip losslessly as
    // `Value::ByteArray`. The inner value is served as raw bytes.
    #[inline]
    fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RawBytesVisitor;

        impl<'de> Visitor<'de> for RawBytesVisitor {
            type Value = Vec<u8>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a raw NBT byte array")
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(v.to_vec())
            }

            fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(v)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut out = Vec::new();
                if let Some(hint) = seq.size_hint() {
                    out.reserve(hint);
                }
                while let Some(byte) = seq.next_element::<i8>()? {
                    out.push(byte.cast_unsigned());
                }
                Ok(out)
            }
        }

        deserializer
            .deserialize_byte_buf(RawBytesVisitor)
            .map(Value::ByteArray)
    }

    #[inline]
    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut out = Vec::new();
        if let Some(hint) = seq.size_hint() {
            out.reserve(hint);
        }

        while let Some(element) = seq.next_element()? {
            out.push(element);
        }

        Ok(Value::List(out))
    }

    #[inline]
    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut out: BTreeMap<BString, Value> = BTreeMap::new();

        while let Some((key, value)) = map.next_entry()? {
            out.insert(key, value);
        }

        Ok(Value::Compound(out))
    }
}
