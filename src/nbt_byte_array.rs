//! A wrapper around [`Vec<u8>`] that (de)serialises as an NBT `ByteArray` tag,
//! giving "plain support" for byte arrays in `#[derive(...)]` structs without
//! pulling in [`serde_bytes`](https://crates.io/crates/serde_bytes).

use std::fmt;
use std::ops::{Deref, DerefMut};

use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A wrapper around [`Vec<u8>`] that (de)serialises as an NBT `ByteArray` tag
/// (tag 7).
///
/// A plain `Vec<i8>` field serialises as a `List` of `Byte` tags, and `Vec<u8>`
/// is not directly serialisable by this crate (unsigned integers are rejected).
/// `NbtByteArray` instead serialises through `serialize_bytes`, which this crate
/// maps to a real `ByteArray` tag, and deserialises the raw bytes back without
/// going through a sequence. It is the byte-array counterpart of
/// [`NbtString`](crate::NbtString).
///
/// # Note on the wire format
///
/// Like [`NbtString`](crate::NbtString), the deserialiser accepts the raw bytes
/// of either a `ByteArray` or a `String` tag (both are byte payloads), so this
/// type is tolerant when reading. When writing it always emits a `ByteArray`
/// tag.
#[derive(Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NbtByteArray(pub Vec<u8>);

impl NbtByteArray {
    /// Creates a new [`NbtByteArray`] from anything convertible into a byte
    /// vector.
    #[inline]
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    /// Returns the raw bytes of the array.
    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consumes the wrapper, returning the inner [`Vec<u8>`].
    #[inline]
    #[must_use]
    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }
}

impl fmt::Debug for NbtByteArray {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "NbtByteArray({:?})", self.0)
    }
}

impl Deref for NbtByteArray {
    type Target = Vec<u8>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NbtByteArray {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<Vec<u8>> for NbtByteArray {
    #[inline]
    fn from(value: Vec<u8>) -> Self {
        Self(value)
    }
}

impl From<NbtByteArray> for Vec<u8> {
    #[inline]
    fn from(value: NbtByteArray) -> Self {
        value.0
    }
}

impl From<&[u8]> for NbtByteArray {
    #[inline]
    fn from(value: &[u8]) -> Self {
        Self(value.to_vec())
    }
}

impl Serialize for NbtByteArray {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // `serialize_bytes` is mapped to an NBT `ByteArray` tag by this crate's
        // binary serializer (and to a `[B;...]` literal by the SNBT serializer).
        serializer.serialize_bytes(&self.0)
    }
}

struct NbtByteArrayVisitor;

impl<'de> Visitor<'de> for NbtByteArrayVisitor {
    type Value = NbtByteArray;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an NBT byte array")
    }

    fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        Ok(NbtByteArray(v.to_vec()))
    }

    fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
        Ok(NbtByteArray(v))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        // Fallback for formats that model a byte array as a sequence of bytes.
        let mut bytes = Vec::new();
        if let Some(hint) = seq.size_hint() {
            bytes.reserve(hint);
        }
        while let Some(byte) = seq.next_element::<i8>()? {
            bytes.push(byte.cast_unsigned());
        }
        Ok(NbtByteArray(bytes))
    }
}

impl<'de> Deserialize<'de> for NbtByteArray {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Request an owned byte buffer. This crate's deserializer hands over the
        // raw bytes of the `ByteArray` (or `String`) tag without going through a
        // sequence of individual `Byte` tags.
        deserializer.deserialize_byte_buf(NbtByteArrayVisitor)
    }
}
