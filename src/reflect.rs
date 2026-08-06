//! The single definition of nbtx's Rust-type → NBT-tag convention, plus the
//! error constructors every codec shares.
//!
//! Three codecs walk the same `facet` shapes and must agree on every answer:
//! the binary one ([`nbt`](crate::nbt)), the textual one
//! ([`snbt`](crate::snbt)), and the direct [`Value`] conversion
//! ([`convert`](crate::convert)). Keeping the rules in one place is what stops
//! them from drifting apart — a `Vec<u8>` has to be a `ByteArray` in all three,
//! a `bstr::BString` has to be a `String` in all three, an enum has to obey the
//! same `#[facet(nbtx::variant_as(...))]` in all three, and so on.
//!
//! This module is deliberately **not** feature-gated: the `Value` conversion is
//! available with no features at all, and it needs the same rules.

use facet::Facet;
use facet_core::{Def, ScalarType, Shape, Variant};
use facet_reflect::Peek;

use crate::error::{
    DiscriminantOutOfRange, MissingVariantAs, UnexpectedType, UnknownField, Unsupported,
};
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

// --- enums ------------------------------------------------------------------

/// What `#[facet(nbtx::variant_as(...))]` must name, for error messages.
const VARIANT_AS_MODES: &str = "`#[facet(nbtx::variant_as(...))]` must name one of `u8`, `i8`, `u16`, `i16`, `u32`, `i32`, `u64`, `i64` or `str`";

/// The wire form of an enum, declared by the mandatory container attribute
/// `#[facet(nbtx::variant_as(<mode>))]`.
///
/// `Str` writes the variant's name as a `String` tag; the eight integer modes
/// write the *active variant's discriminant* as a fixed-width NBT scalar. The
/// mode is a wire-format choice only: it is independent of the `#[repr(...)]`
/// that gives the enum its Rust memory layout (facet's derive requires one of
/// those anyway), so a `#[repr(u8)]` enum can perfectly well be sent as an
/// `i32`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VariantAs {
    /// The variant's [effective name](facet_core::Variant::effective_name), as a
    /// `String` tag.
    Str,
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
}

impl VariantAs {
    /// Reads the mode `shape`'s enum declared, erroring if it declared none.
    ///
    /// The attribute stores the *type* named in the parentheses as a
    /// `&'static Shape` (facet's `shape_type` attribute kind), so the mode is
    /// recovered by comparing shape ids. `usize`/`isize` are deliberately not
    /// accepted: their width varies by target, and a document that decodes
    /// differently on a 32-bit machine is not a wire format.
    pub(crate) fn of(shape: &Shape) -> Result<VariantAs, Error> {
        let attr = shape
            .attributes
            .iter()
            .find(|a| a.ns == Some("nbtx") && a.key == "variant_as")
            .ok_or(Error::MissingVariantAs(MissingVariantAs {
                container: shape.type_identifier,
            }))?;
        let mode = attr
            .get_as::<Shape>()
            .ok_or_else(|| unsupported(VARIANT_AS_MODES))?;

        let id = mode.id;
        Ok(if id == <str as Facet>::SHAPE.id {
            VariantAs::Str
        } else if id == <u8 as Facet>::SHAPE.id {
            VariantAs::U8
        } else if id == <i8 as Facet>::SHAPE.id {
            VariantAs::I8
        } else if id == <u16 as Facet>::SHAPE.id {
            VariantAs::U16
        } else if id == <i16 as Facet>::SHAPE.id {
            VariantAs::I16
        } else if id == <u32 as Facet>::SHAPE.id {
            VariantAs::U32
        } else if id == <i32 as Facet>::SHAPE.id {
            VariantAs::I32
        } else if id == <u64 as Facet>::SHAPE.id {
            VariantAs::U64
        } else if id == <i64 as Facet>::SHAPE.id {
            VariantAs::I64
        } else {
            return Err(unsupported(VARIANT_AS_MODES));
        })
    }

    /// The NBT tag this mode's values are carried in.
    pub(crate) fn tag(self) -> FieldType {
        match self {
            VariantAs::Str => FieldType::String,
            VariantAs::U8 | VariantAs::I8 => FieldType::Byte,
            VariantAs::U16 | VariantAs::I16 => FieldType::Short,
            VariantAs::U32 | VariantAs::I32 => FieldType::Int,
            VariantAs::U64 | VariantAs::I64 => FieldType::Long,
        }
    }

    /// The mode as it is spelled in the attribute.
    pub(crate) fn name(self) -> &'static str {
        match self {
            VariantAs::Str => "str",
            VariantAs::U8 => "u8",
            VariantAs::I8 => "i8",
            VariantAs::U16 => "u16",
            VariantAs::I16 => "i16",
            VariantAs::U32 => "u32",
            VariantAs::I32 => "i32",
            VariantAs::U64 => "u64",
            VariantAs::I64 => "i64",
        }
    }

    /// Narrows a discriminant to this mode's width, as the *signed* value of the
    /// NBT tag that carries it, or `None` if it does not fit.
    ///
    /// An unsigned mode keeps the bit pattern rather than the number: mode `u8`
    /// sends discriminant 200 as `Byte(-56)`, the same convention a plain `u8`
    /// field already uses (see [`scalar_tag`]). [`Self::widen`] undoes it.
    fn narrow(self, disc: i64) -> Option<i64> {
        Some(match self {
            // Unreachable: `enum_wire` answers `Str` from the variant's name and
            // never asks for a discriminant. Reported as "does not fit" rather
            // than panicking if that ever stops being true.
            VariantAs::Str => return None,
            VariantAs::U8 => i64::from(u8::try_from(disc).ok()?.cast_signed()),
            VariantAs::I8 => i64::from(i8::try_from(disc).ok()?),
            VariantAs::U16 => i64::from(u16::try_from(disc).ok()?.cast_signed()),
            VariantAs::I16 => i64::from(i16::try_from(disc).ok()?),
            VariantAs::U32 => i64::from(u32::try_from(disc).ok()?.cast_signed()),
            VariantAs::I32 => i64::from(i32::try_from(disc).ok()?),
            // 64 bits wide either way: the whole bit pattern already fits, and
            // `i64` is how facet reports every discriminant in the first place.
            VariantAs::U64 | VariantAs::I64 => disc,
        })
    }

    /// Widens a scalar tag's signed value back into a discriminant, undoing
    /// [`Self::narrow`].
    ///
    /// The tag types are all signed, so an unsigned mode has to reinterpret the
    /// bit pattern; a signed one only has to sign-extend, which the reader's own
    /// `i8`/`i16`/`i32` already did.
    pub(crate) fn widen(self, raw: i64) -> i64 {
        match self {
            VariantAs::U8 => i64::from((raw as i8).cast_unsigned()),
            VariantAs::U16 => i64::from((raw as i16).cast_unsigned()),
            VariantAs::U32 => i64::from((raw as i32).cast_unsigned()),
            VariantAs::I8 => i64::from(raw as i8),
            VariantAs::I16 => i64::from(raw as i16),
            VariantAs::I32 => i64::from(raw as i32),
            // 64 bits wide either way, and `Str` (which never gets here) has no
            // discriminant form at all.
            VariantAs::Str | VariantAs::U64 | VariantAs::I64 => raw,
        }
    }
}

/// How one concrete enum value is written, once its mode has been applied.
pub(crate) enum EnumWire {
    /// `str` mode: write this name as a `String` tag.
    Name(&'static str),
    /// An integer mode: write this already-narrowed, signed value as `tag`.
    Int(FieldType, i64),
}

/// The NBT tag an enum's values carry, from its declared mode alone.
///
/// Needs no value, so it also answers for the element type of an empty list.
///
/// Only the binary codec's `tag_of`/`tag_of_shape` (`nbt::ser`) need a tag
/// ahead of a value: the SNBT writer and the `Value` conversion both go
/// through [`enum_wire`] instead, which already has a `Peek` in hand. Gated on
/// `nbt` so it is not dead code when that feature is off.
#[cfg(feature = "nbt")]
pub(crate) fn enum_tag(shape: &Shape) -> Result<FieldType, Error> {
    Ok(VariantAs::of(shape)?.tag())
}

/// Resolves one concrete enum value into what the codecs must write.
///
/// Shared by all three serializers so the mode, the rename and the
/// discriminant-range check are applied identically by each.
pub(crate) fn enum_wire(peek: Peek) -> Result<EnumWire, Error> {
    let shape = peek.shape();
    let en = peek.into_enum().map_err(reflect_err)?;
    let variant = en.active_variant().map_err(reflect_err)?;
    if !variant.data.fields.is_empty() {
        return Err(unsupported(
            "serializing enums with data (other than `Value`) is not supported",
        ));
    }

    let mode = VariantAs::of(shape)?;
    if mode == VariantAs::Str {
        // `#[facet(rename = "...")]` on the variant wins over its Rust name,
        // exactly as it does for a struct field's key.
        return Ok(EnumWire::Name(variant.effective_name()));
    }
    Ok(EnumWire::Int(
        mode.tag(),
        discriminant(shape, mode, variant)?,
    ))
}

/// The active variant's discriminant, narrowed to `mode`'s width.
///
/// Read from [`Variant::discriminant`] rather than
/// [`PeekEnum::discriminant`](facet_reflect::PeekEnum::discriminant) so that the
/// value written is by construction the one `Partial::select_variant` matches
/// against when reading it back (and so that an enum whose layout does not carry
/// a readable discriminant errors instead of panicking). Both report the custom
/// numbering of `enum Foo { A = 5 }`, which is how a per-variant override is
/// spelled: nbtx has no attribute of its own for it.
fn discriminant(shape: &Shape, mode: VariantAs, variant: &Variant) -> Result<i64, Error> {
    let out_of_range = |disc| {
        Error::DiscriminantOutOfRange(DiscriminantOutOfRange {
            container: shape.type_identifier,
            variant: variant.name,
            discriminant: disc,
            mode: mode.name(),
        })
    };
    let disc = variant
        .discriminant
        .ok_or_else(|| unsupported("this enum's variants have no discriminant to serialize"))?;
    mode.narrow(disc).ok_or_else(|| out_of_range(disc))
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
