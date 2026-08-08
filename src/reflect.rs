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
use facet_core::{Def, Field, ScalarType, Shape, Variant};
use facet_reflect::{Partial, Peek};

use crate::error::{
    DiscriminantOutOfRange, InvalidLenientWidth, LenientWidthOutOfRange, MissingVariantAs,
    UnexpectedType, UnknownField, Unsupported,
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
            .find(|a| a.ns() == Some("nbtx") && a.key() == "variant_as")
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

// --- lenient width ----------------------------------------------------------

/// What `#[facet(nbtx::lenient_width(...))]` may name, for error messages.
const LENIENT_WIDTH_TYPES: &str = "`#[facet(nbtx::lenient_width(...))]` must name one or more of `i8`, `i16`, `i32`, `i64`, `f32` or `f64`";

/// One NBT scalar exactly as it arrived, before any widening.
///
/// The six variants are the six native NBT scalar tags, and they are also
/// exactly the six types `#[facet(nbtx::lenient_width(...))]` may name — the
/// attribute names *wire* types, so the two lists cannot drift apart.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum WireScalar {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
}

impl std::fmt::Display for WireScalar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WireScalar::Byte(v) => write!(f, "{v}"),
            WireScalar::Short(v) => write!(f, "{v}"),
            WireScalar::Int(v) => write!(f, "{v}"),
            WireScalar::Long(v) => write!(f, "{v}"),
            WireScalar::Float(v) => write!(f, "{v}"),
            WireScalar::Double(v) => write!(f, "{v}"),
        }
    }
}

/// Is `v` exactly representable in a binary float with `mantissa` significand
/// bits (24 for `f32`, 53 for `f64`)?
///
/// The test is on the *significant* bits — the span from the highest set bit
/// down to the lowest — not on the magnitude: 2^62 needs one mantissa bit and
/// converts to an `f32` exactly, while 16 777 217 needs 25 and does not. No
/// exponent check is needed because both float types reach far past `i64`'s
/// range.
fn int_fits_float(v: i64, mantissa: u32) -> bool {
    let m = v.unsigned_abs();
    if m == 0 {
        return true;
    }
    (u64::BITS - m.leading_zeros()) - m.trailing_zeros() <= mantissa
}

/// `2^63`, the first `f64` above `i64::MAX`. Exactly representable, and the
/// bound `float_to_i64` range-checks against: `as` saturates rather than
/// wrapping, so an out-of-range float would otherwise come back as `i64::MAX`.
const I64_LIMIT: f64 = 9_223_372_036_854_775_808.0;

/// The exact integer a float denotes, or `None` if it has a fractional part, is
/// not finite, or lies outside `i64`.
fn float_to_i64(v: f64) -> Option<i64> {
    if !v.is_finite() || v.fract() != 0.0 || !(-I64_LIMIT..I64_LIMIT).contains(&v) {
        return None;
    }
    Some(v as i64)
}

impl WireScalar {
    /// The NBT tag this value arrived in.
    pub(crate) fn tag(self) -> FieldType {
        match self {
            WireScalar::Byte(_) => FieldType::Byte,
            WireScalar::Short(_) => FieldType::Short,
            WireScalar::Int(_) => FieldType::Int,
            WireScalar::Long(_) => FieldType::Long,
            WireScalar::Float(_) => FieldType::Float,
            WireScalar::Double(_) => FieldType::Double,
        }
    }

    /// The exact integer this value denotes, or `None` for a float that is not
    /// a whole number (or not finite).
    fn to_i64(self) -> Option<i64> {
        Some(match self {
            WireScalar::Byte(v) => i64::from(v),
            WireScalar::Short(v) => i64::from(v),
            WireScalar::Int(v) => i64::from(v),
            WireScalar::Long(v) => v,
            WireScalar::Float(v) => float_to_i64(f64::from(v))?,
            WireScalar::Double(v) => float_to_i64(v)?,
        })
    }

    /// This value as `T`, or `None` if `T` cannot hold it.
    fn to_int<T: TryFrom<i64>>(self) -> Option<T> {
        T::try_from(self.to_i64()?).ok()
    }

    /// This value as an `f32`, or `None` if `f32` cannot hold it exactly.
    fn to_f32(self) -> Option<f32> {
        Some(match self {
            WireScalar::Byte(v) => f32::from(v),
            WireScalar::Short(v) => f32::from(v),
            WireScalar::Float(v) => v,
            WireScalar::Int(_) | WireScalar::Long(_) => {
                let v = self.to_i64()?;
                if !int_fits_float(v, f32::MANTISSA_DIGITS) {
                    return None;
                }
                #[allow(clippy::cast_precision_loss)]
                let out = v as f32;
                out
            }
            WireScalar::Double(v) => {
                let narrowed = v as f32;
                // Exact equality is the whole point: the question is whether
                // `f32` holds this `f64` *bit for bit*, and any tolerance would
                // be the silent rounding this attribute exists to avoid. A
                // `NaN` is never equal to itself yet does survive the
                // narrowing, so it is admitted explicitly.
                #[allow(clippy::float_cmp)]
                let exact = f64::from(narrowed) == v;
                if !v.is_nan() && !exact {
                    return None;
                }
                narrowed
            }
        })
    }

    /// This value as an `f64`, or `None` if `f64` cannot hold it exactly.
    fn to_f64(self) -> Option<f64> {
        Some(match self {
            WireScalar::Byte(v) => f64::from(v),
            WireScalar::Short(v) => f64::from(v),
            WireScalar::Int(v) => f64::from(v),
            WireScalar::Float(v) => f64::from(v),
            WireScalar::Double(v) => v,
            WireScalar::Long(v) => {
                if !int_fits_float(v, f64::MANTISSA_DIGITS) {
                    return None;
                }
                #[allow(clippy::cast_precision_loss)]
                let out = v as f64;
                out
            }
        })
    }

    /// Builds the "allowed tag, unrepresentable value" error.
    fn out_of_range(self, target: &'static str) -> Error {
        Error::LenientWidthOutOfRange(LenientWidthOutOfRange {
            value: self.to_string(),
            from: self.tag(),
            target,
        })
    }
}

/// The wire scalar a [`Value`] node carries, or `None` for the six non-scalar
/// tags (which `lenient_width` never names).
pub(crate) fn wire_scalar(value: &Value) -> Option<WireScalar> {
    Some(match *value {
        Value::Byte(v) => WireScalar::Byte(v),
        Value::Short(v) => WireScalar::Short(v),
        Value::Int(v) => WireScalar::Int(v),
        Value::Long(v) => WireScalar::Long(v),
        Value::Float(v) => WireScalar::Float(v),
        Value::Double(v) => WireScalar::Double(v),
        _ => return None,
    })
}

/// Stores `wire` at its own width — the tag it arrived in *is* the target type,
/// so nothing is converted and nothing can fail to fit.
#[cfg(feature = "snbt")]
pub(crate) fn set_wire(p: Partial<'_, true>, wire: WireScalar) -> Result<Partial<'_, true>, Error> {
    match wire {
        WireScalar::Byte(v) => p.set(v),
        WireScalar::Short(v) => p.set(v),
        WireScalar::Int(v) => p.set(v),
        WireScalar::Long(v) => p.set(v),
        WireScalar::Float(v) => p.set(v),
        WireScalar::Double(v) => p.set(v),
    }
    .map_err(reflect_err)
}

/// Widens `wire` into a field of type `target` and stores it, or reports
/// [`Error::LenientWidthOutOfRange`] if the value does not survive the trip.
///
/// Every conversion here is lossless by construction: an integer only reaches a
/// narrower integer if it is in range, only reaches a float if that float
/// converts back to the very same integer, and a float only reaches an integer
/// if it is whole. Nothing is truncated, wrapped or rounded.
pub(crate) fn set_lenient(
    p: Partial<'_, true>,
    target: ScalarType,
    wire: WireScalar,
) -> Result<Partial<'_, true>, Error> {
    macro_rules! int {
        ($t:ty) => {
            wire.to_int::<$t>()
                .ok_or_else(|| wire.out_of_range(stringify!($t)))?
        };
    }
    match target {
        ScalarType::I8 => p.set(int!(i8)),
        ScalarType::I16 => p.set(int!(i16)),
        ScalarType::I32 => p.set(int!(i32)),
        ScalarType::I64 => p.set(int!(i64)),
        ScalarType::F32 => p.set(wire.to_f32().ok_or_else(|| wire.out_of_range("f32"))?),
        ScalarType::F64 => p.set(wire.to_f64().ok_or_else(|| wire.out_of_range("f64"))?),
        // Unreachable: `Lenient::of_field` refuses every other leaf type before
        // a codec can get here.
        _ => return Err(unsupported(LENIENT_WIDTH_TYPES)),
    }
    .map_err(reflect_err)
}

/// Widens `wire` into the discriminant of an enum whose declared mode is `mode`.
///
/// The value is first converted to the *signed* integer type of the mode's own
/// tag — losslessly, as everywhere else — and then read exactly as a natural tag
/// of that width would be, so an unsigned mode still reinterprets the bit
/// pattern through [`VariantAs::widen`] and a document cannot select a different
/// variant by changing which width it wrote the number in.
pub(crate) fn lenient_discriminant(wire: WireScalar, mode: VariantAs) -> Result<i64, Error> {
    macro_rules! int {
        ($t:ty) => {
            wire.to_int::<$t>()
                .ok_or_else(|| wire.out_of_range(stringify!($t)))?
        };
    }
    let raw = match mode.tag() {
        FieldType::Byte => i64::from(int!(i8)),
        FieldType::Short => i64::from(int!(i16)),
        FieldType::Int => i64::from(int!(i32)),
        // `Str` never reaches here: `Lenient::of_enum` rejects the attribute on
        // a `variant_as(str)` enum outright.
        _ => int!(i64),
    };
    Ok(mode.widen(raw))
}

/// The set of *extra* NBT scalar tags a field (or an enum's discriminant) will
/// accept on decode, from `#[facet(nbtx::lenient_width(...))]`.
///
/// A bit set rather than the attribute's `&[&Shape]` so that it is one `Copy`
/// byte: the binary deserializer threads it down through every element of a
/// `Vec` and every level of an `Option`, on the same recursive path whose frame
/// size the depth guard is sized against.
///
/// [`Lenient::NONE`] — no bits — is the "no attribute" case. A *declared*
/// attribute always sets at least one bit: facet rejects an empty list at
/// macro-expansion time, and a list naming a type that is not one of the six is
/// refused by [`Lenient::parse`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Lenient(u8);

impl Lenient {
    /// No `lenient_width` declaration: only the field's own tag is accepted.
    pub(crate) const NONE: Lenient = Lenient(0);

    /// The bit standing for one scalar tag.
    fn bit(tag: FieldType) -> u8 {
        match tag {
            FieldType::Byte => 1,
            FieldType::Short => 2,
            FieldType::Int => 4,
            FieldType::Long => 8,
            FieldType::Float => 16,
            FieldType::Double => 32,
            _ => 0,
        }
    }

    /// Does this declaration widen `tag` in?
    ///
    /// Answers `false` for [`Lenient::NONE`] and for every non-scalar tag, so a
    /// caller can ask without first checking whether an attribute was present.
    pub(crate) fn accepts(self, tag: FieldType) -> bool {
        let bit = Lenient::bit(tag);
        bit != 0 && self.0 & bit != 0
    }

    /// Decodes the attribute's payload — a `list(shape_type)`, so a slice of
    /// `&'static Shape` — into the tag set it names.
    ///
    /// Recovered by comparing shape ids, the same technique
    /// [`VariantAs::of`] uses. `u8`/`bool` are deliberately absent even though
    /// both are carried in a `Byte`: the attribute names the *six NBT scalar
    /// wire types*, and `i8` already stands for that tag.
    fn parse(attr: &facet_core::Attr) -> Result<Lenient, Error> {
        let decoded = attr
            .get_as::<crate::Attr>()
            .ok_or_else(|| unsupported(LENIENT_WIDTH_TYPES))?;
        let crate::Attr::LenientWidth(shapes) = decoded else {
            return Err(unsupported(LENIENT_WIDTH_TYPES));
        };
        let mut bits = 0;
        for shape in shapes.iter().copied() {
            let id = shape.id;
            let tag = if id == <i8 as Facet>::SHAPE.id {
                FieldType::Byte
            } else if id == <i16 as Facet>::SHAPE.id {
                FieldType::Short
            } else if id == <i32 as Facet>::SHAPE.id {
                FieldType::Int
            } else if id == <i64 as Facet>::SHAPE.id {
                FieldType::Long
            } else if id == <f32 as Facet>::SHAPE.id {
                FieldType::Float
            } else if id == <f64 as Facet>::SHAPE.id {
                FieldType::Double
            } else {
                return Err(unsupported(LENIENT_WIDTH_TYPES));
            };
            bits |= Lenient::bit(tag);
        }
        Ok(Lenient(bits))
    }

    /// Reads the declaration on `field` of struct `container`, validating that
    /// the field is something widening can apply to.
    ///
    /// This is where the placement rule is enforced, because it is the first
    /// place that can be: the attribute grammar sees the attribute's own tokens
    /// and nothing about the type it was written on, so a `lenient_width` on a
    /// nested compound cannot be caught at derive time. Every decoder calls this
    /// as it enters a field, so the error surfaces on first decode.
    pub(crate) fn of_field(container: &Shape, field: &Field) -> Result<Lenient, Error> {
        let Some(attr) = field.get_attr(Some("nbtx"), "lenient_width") else {
            return Ok(Lenient::NONE);
        };
        let lenient = Lenient::parse(attr)?;
        if lenient_leaf(field.shape()).is_none() {
            return Err(Error::InvalidLenientWidth(InvalidLenientWidth {
                container: container.type_identifier,
                field: field.name,
                reason: "this field has no scalar to widen",
            }));
        }
        Ok(lenient)
    }

    /// Reads the declaration on an enum, beside its `variant_as(<mode>)`.
    ///
    /// Written on the container rather than per variant: it widens the tag the
    /// *discriminant* arrives in, which is a property of the whole enum, exactly
    /// like the mode it is qualifying.
    pub(crate) fn of_enum(shape: &Shape, mode: VariantAs) -> Result<Lenient, Error> {
        let Some(attr) = shape
            .attributes
            .iter()
            .find(|a| a.ns() == Some("nbtx") && a.key() == "lenient_width")
        else {
            return Ok(Lenient::NONE);
        };
        let lenient = Lenient::parse(attr)?;
        if mode == VariantAs::Str {
            return Err(Error::InvalidLenientWidth(InvalidLenientWidth {
                container: shape.type_identifier,
                field: "<container>",
                reason: "its `variant_as(str)` mode carries variant names, not numbers",
            }));
        }
        Ok(lenient)
    }
}

/// The scalar a `lenient_width` field ultimately widens, or `None` if the field
/// has no such leaf and the attribute is therefore misplaced.
///
/// `Option`, `Vec`/`[T; N]` and slices are peeled — one declaration widens every
/// element uniformly — and everything else must already be one of the six NBT
/// scalars. A struct, a map, an enum, a `bool`, a `u8`, a string and a dynamic
/// [`Value`] all stop the walk with `None`: none of them is a fixed-width number
/// that another width could stand in for.
fn lenient_leaf(shape: &Shape) -> Option<ScalarType> {
    let mut shape = shape;
    loop {
        // A `BString` reflects as a `Def::List<u8>` but holds a string, and a
        // scalar can carry a `Def` of its own, so both are settled before the
        // container peel below — the same order every codec dispatches in.
        if is_bstring(shape) || is_value(shape) {
            return None;
        }
        if let Some(scalar) = ScalarType::try_from_shape(shape) {
            return matches!(
                scalar,
                ScalarType::I8
                    | ScalarType::I16
                    | ScalarType::I32
                    | ScalarType::I64
                    | ScalarType::F32
                    | ScalarType::F64
            )
            .then_some(scalar);
        }
        shape = match shape.def {
            Def::Option(def) => def.t(),
            Def::List(def) => def.t(),
            Def::Array(def) => def.t(),
            Def::Slice(def) => def.t(),
            _ => return None,
        };
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
