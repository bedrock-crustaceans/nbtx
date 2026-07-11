//! A wrapper around [`bstr::BString`] that (de)serialises as a raw NBT String
//! tag, giving "plain support" for non-UTF-8 NBT strings in `#[derive(...)]`
//! structs.

use std::fmt;
use std::ops::{Deref, DerefMut};

use bstr::BString;
use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::nbt::ser::RAW_STRING_TOKEN;

/// A byte-string wrapper around [`bstr::BString`] that (de)serialises losslessly
/// as an NBT String tag, even when the bytes are not valid UTF-8.
///
/// NBT strings are not guaranteed to be valid UTF-8 (Java uses MUTF-8 and
/// Bedrock allows arbitrary bytes). A plain [`String`] field validates the
/// bytes as UTF-8 and errors on invalid data; using `NbtString` (or
/// [`bstr::BString`] behind it) instead hands the raw bytes over untouched.
///
/// # Serialization vs. a bare [`bstr::BString`]
///
/// A bare [`bstr::BString`] field serialises through `serialize_bytes`, which
/// this crate maps to an NBT `ByteArray` tag, so it does not round-trip as a
/// `String` tag. `NbtString` instead emits a magic newtype token that this
/// crate's serializer recognises and writes as a real `String` tag. Prefer
/// `NbtString` whenever the field represents an NBT string.
#[derive(Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NbtString(pub BString);

impl NbtString {
    /// Creates a new [`NbtString`] from anything convertible into a byte
    /// vector.
    #[inline]
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(BString::new(bytes.into()))
    }

    /// Returns the raw bytes of the string.
    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }

    /// Consumes the wrapper, returning the inner [`bstr::BString`].
    #[inline]
    #[must_use]
    pub fn into_inner(self) -> BString {
        self.0
    }
}

impl fmt::Debug for NbtString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "NbtString({:?})", self.0)
    }
}

impl fmt::Display for NbtString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl Deref for NbtString {
    type Target = BString;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NbtString {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<BString> for NbtString {
    #[inline]
    fn from(value: BString) -> Self {
        Self(value)
    }
}

impl From<NbtString> for BString {
    #[inline]
    fn from(value: NbtString) -> Self {
        value.0
    }
}

impl From<Vec<u8>> for NbtString {
    #[inline]
    fn from(value: Vec<u8>) -> Self {
        Self(BString::new(value))
    }
}

impl From<String> for NbtString {
    #[inline]
    fn from(value: String) -> Self {
        Self(BString::from(value))
    }
}

impl From<&str> for NbtString {
    #[inline]
    fn from(value: &str) -> Self {
        Self(BString::from(value))
    }
}

impl Serialize for NbtString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // The inner `BString` serialises through `serialize_bytes`; wrapping it
        // in the magic newtype token tells this crate's serializer to write the
        // bytes as a raw NBT String tag instead of a ByteArray.
        serializer.serialize_newtype_struct(RAW_STRING_TOKEN, &self.0)
    }
}

struct NbtStringVisitor;

impl<'de> Visitor<'de> for NbtStringVisitor {
    type Value = NbtString;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a raw NBT string (arbitrary bytes)")
    }

    fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        Ok(NbtString(BString::from(v.to_vec())))
    }

    fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
        Ok(NbtString(BString::from(v)))
    }

    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(NbtString(BString::from(v)))
    }

    fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
        Ok(NbtString(BString::from(v)))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        // Fallback for formats that model a byte string as a sequence of bytes.
        let mut bytes = Vec::new();
        while let Some(byte) = seq.next_element::<u8>()? {
            bytes.push(byte);
        }
        Ok(NbtString(BString::from(bytes)))
    }
}

impl<'de> Deserialize<'de> for NbtString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Request an owned byte buffer without UTF-8 validation. This crate's
        // deserializer recognises the String tag and hands over the raw bytes.
        deserializer.deserialize_byte_buf(NbtStringVisitor)
    }
}
