//! The single definition of nbtx's Rust-type → NBT-tag convention, plus the
//! error constructors every codec shares.
//!
//! Three codecs walk the same `facet` shapes and must agree on every answer:
//! the binary one ([`nbt`](crate::nbt)), the textual one
//! ([`snbt`](crate::snbt)), and the direct [`Value`] conversion
//! ([`convert`](crate::convert)). Keeping the rules in one place is what stops
//! them from drifting apart — a `Vec<u8>` has to be a `ByteArray` in all three,
//! a `bstr::BString` has to be a `String` in all three, and so on.
//!
//! This module is deliberately **not** feature-gated: the `Value` conversion is
//! available with no features at all, and it needs the same rules.

use facet::Facet;
use facet_core::{Def, ScalarType, Shape};
use facet_reflect::Peek;

use crate::error::{UnexpectedType, UnknownField, Unsupported};
use crate::{Error, FieldType, Value};

/// Wraps a `facet_reflect` failure, which only reports as a `Display` string.
pub(crate) fn reflect_err(e: impl std::fmt::Display) -> Error {
    Error::Other(e.to_string())
}

/// Builds the "this cannot be represented in NBT" error.
pub(crate) fn unsupported(op: &'static str) -> Error {
    Error::Unsupported(Unsupported {
        op,
        #[cfg(feature = "error-context")]
        at: String::from("unknown"),
        #[cfg(feature = "error-context")]
        index: None,
    })
}

/// Builds the "wrong tag for this target type" error.
pub(crate) fn unexpected_type(expected: FieldType, actual: FieldType) -> Error {
    Error::UnexpectedType(UnexpectedType {
        expected,
        actual,
        #[cfg(feature = "error-context")]
        at: String::from("unknown"),
        #[cfg(feature = "error-context")]
        index: None,
    })
}

/// Builds the `unknown compound key` error for `shape`'s struct.
///
/// `key` is raw bytes because NBT keys are not guaranteed to be UTF-8; it is
/// rendered lossily for the message.
pub(crate) fn unknown_field(shape: &Shape, key: &[u8]) -> Error {
    Error::UnknownField(UnknownField {
        field: bstr::BStr::new(key).to_string(),
        container: shape.type_identifier,
    })
}

/// Returns `true` if the shape is the dynamic [`Value`] type.
///
/// Matched by `Shape::id`: `Value` is not generic, so one id identifies it
/// exactly, and a user type that merely looks like it is unaffected.
pub(crate) fn is_value(shape: &Shape) -> bool {
    shape.id == <Value as Facet>::SHAPE.id
}

/// Returns `true` if the shape is `bstr::BString`/`BStr`.
///
/// These reflect as a `Def::List<u8>`, but semantically hold an NBT *string*
/// (raw, possibly non-UTF-8 bytes), so they must map to the `String` tag rather
/// than `ByteArray`.
pub(crate) fn is_bstring(shape: &Shape) -> bool {
    matches!(shape.type_identifier, "BString" | "BStr")
}

/// The NBT tag a concrete [`Value`] node carries.
///
/// Infallible by construction — every `Value` variant *is* a tag — which is why
/// this exists alongside `FieldType::try_from`: the latter parses an arbitrary
/// wire byte and can fail, and it is only compiled with the `nbt` feature.
pub(crate) fn value_tag(value: &Value) -> FieldType {
    match value {
        Value::Byte(_) => FieldType::Byte,
        Value::Short(_) => FieldType::Short,
        Value::Int(_) => FieldType::Int,
        Value::Long(_) => FieldType::Long,
        Value::Float(_) => FieldType::Float,
        Value::Double(_) => FieldType::Double,
        Value::ByteArray(_) => FieldType::ByteArray,
        Value::String(_) => FieldType::String,
        Value::List(_) => FieldType::List,
        Value::Compound(_) => FieldType::Compound,
        Value::IntArray(_) => FieldType::IntArray,
        Value::LongArray(_) => FieldType::LongArray,
    }
}

/// Maps a *bare* scalar type to its NBT tag.
pub(crate) fn scalar_tag(scalar: ScalarType) -> Option<FieldType> {
    Some(match scalar {
        // A bare `u8` is a `Byte` tag, matching `nbt::de::read_scalar` (which
        // reads one back with `cast_unsigned`) and both halves of the SNBT
        // codec. Only *bare* scalars: a `Vec<u8>`/`[u8; N]` is a `ByteArray`,
        // decided by `list_tag` on the element shape, not here.
        ScalarType::Bool | ScalarType::I8 | ScalarType::U8 => FieldType::Byte,
        ScalarType::I16 => FieldType::Short,
        ScalarType::I32 => FieldType::Int,
        ScalarType::I64 => FieldType::Long,
        ScalarType::F32 => FieldType::Float,
        ScalarType::F64 => FieldType::Double,
        ScalarType::Str | ScalarType::String | ScalarType::CowStr => FieldType::String,
        _ => return None,
    })
}

/// Maps a list/array element shape to the tag of the *containing* sequence.
pub(crate) fn list_tag(elem: &Shape) -> FieldType {
    let id = elem.id;
    if id == <u8 as Facet>::SHAPE.id {
        FieldType::ByteArray
    } else if id == <i32 as Facet>::SHAPE.id {
        FieldType::IntArray
    } else if id == <i64 as Facet>::SHAPE.id {
        FieldType::LongArray
    } else {
        FieldType::List
    }
}

/// If `peek` is an `Option`, returns `None` for `None` (a field to skip) and the
/// inner peek for `Some`. Non-option peeks are returned unchanged.
pub(crate) fn unwrap_option<'m, 'f>(peek: Peek<'m, 'f>) -> Result<Option<Peek<'m, 'f>>, Error> {
    if let Def::Option(_) = peek.shape().def {
        let opt = peek.into_option().map_err(reflect_err)?;
        Ok(opt.value())
    } else {
        Ok(Some(peek))
    }
}
