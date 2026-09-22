//! NBT (binary) serialization, driven by `facet` reflection.
//!
//! The serializer walks a [`facet_reflect::Peek`] and emits NBT bytes. The NBT
//! tag of each node is derived from the Rust type:
//!
//! * `bool`/`i8`/`u8` → `Byte`, `i16` → `Short`, `i32` → `Int`, `i64` → `Long`,
//!   `f32` → `Float`, `f64` → `Double`
//! * `String`/`&str` → `String`
//! * `Vec<u8>`/`[u8; N]` → `ByteArray`, `Vec<i32>`/`[i32; N]` → `IntArray`,
//!   `Vec<i64>`/`[i64; N]` → `LongArray`, any other list/array → `List`
//! * struct/map → `Compound`
//! * a unit enum variant → whatever its mandatory
//!   `#[facet(nbtx::variant_as(<mode>))]` declares: its (rename-aware) name as a
//!   `String`, or its discriminant as a `Byte`/`Short`/`Int`/`Long`. See
//!   [`VariantAs`](crate::reflect::VariantAs); an enum without the attribute is
//!   an [`Error::MissingVariantAs`](crate::Error::MissingVariantAs).
//!
//! [`lenient_width`](crate::Attr::LenientWidth) is deliberately *not* consulted
//! here. It is a decode-only tolerance, so a field is always written with its
//! own tag and re-encoding a leniently decoded document normalises it.
//!
//! The dynamic [`Value`] type is special-cased: whenever a node's shape is
//! `Value`, its real Rust value is read via a downcast and encoded directly,
//! preserving the exact tag of every child (so `ByteArray`/`IntArray`/
//! `LongArray`/`List` stay distinct) and non-UTF-8 `BString` payloads. A
//! [`ValueList`] field is special-cased the same way: it is already the wire
//! form of a `TAG_List`, element type included, so it is written verbatim —
//! which is how an *empty* typed list keeps its element type.
//!
//! A `Vec<Value>` field is the one sequence whose elements can disagree about
//! their tag. A list has a single element-type byte, so a mixture has no
//! encoding at all and is reported as
//! [`Error::HeterogeneousList`](crate::Error::HeterogeneousList) rather than
//! written into a stream that would decode as something else.

use std::marker::PhantomData;

use byteorder::WriteBytesExt;
use facet::Facet;
use facet_core::{Def, ScalarType, Type, UserType};
use facet_reflect::Peek;

use crate::named;
use crate::nbt::io;
// The type→tag convention and the error constructors are shared with the SNBT
// codec and the `Value` conversion, so they live in `crate::reflect`.
use crate::reflect::{
    EnumWire, enum_tag, enum_wire, is_bstring, is_value, is_value_list, list_tag, reflect_err,
    scalar_tag, tag_of_shape, unsupported, unwrap_option, value_tag,
};
use crate::{
    BigEndian, EndiannessImpl, Error, FieldType, LittleEndian, Value, ValueList, VarintEndian,
};

/// Determines the NBT tag for a concrete value.
fn tag_of(peek: Peek) -> Result<FieldType, Error> {
    let shape = peek.shape();
    if is_value(shape) {
        let v: &Value = peek.get::<Value>().map_err(reflect_err)?;
        return Ok(value_tag(v));
    }
    if is_value_list(shape) {
        return Ok(FieldType::List);
    }
    if is_bstring(shape) {
        return Ok(FieldType::String);
    }
    if let Some(scalar) = ScalarType::try_from_shape(shape) {
        return scalar_tag(scalar)
            .ok_or_else(|| unsupported("serialization of this scalar type is not supported"));
    }
    match shape.def {
        Def::List(def) => Ok(list_tag(def.t())),
        Def::Array(def) => Ok(list_tag(def.t())),
        Def::Slice(def) => Ok(list_tag(def.t())),
        Def::Map(_) => Ok(FieldType::Compound),
        Def::Option(_) => match unwrap_option(peek)? {
            Some(inner) => tag_of(inner),
            None => Err(unsupported("cannot serialize a `None` value here")),
        },
        _ => match shape.ty {
            Type::User(UserType::Struct(_)) => Ok(FieldType::Compound),
            Type::User(UserType::Enum(_)) => enum_tag(shape),
            _ => Err(unsupported("serialization of this type is not supported")),
        },
    }
}

/// Writes the *payload* of `peek` (the tag byte, if any, is written by the
/// caller).
///
/// `depth` is the number of containers already entered; it is checked against
/// [`MAX_DEPTH`](crate::MAX_DEPTH) before recursing so that a deeply nested Rust
/// value fails with an error rather than overflowing the stack.
fn write_payload<F: EndiannessImpl, W: WriteBytesExt>(
    w: &mut W,
    peek: Peek,
    depth: usize,
) -> Result<(), Error> {
    let shape = peek.shape();

    // Dynamic value: encode directly, preserving exact tags and raw bytes.
    if is_value(shape) {
        let v: &Value = peek.get::<Value>().map_err(reflect_err)?;
        return io::write_value::<F, W>(w, v, depth);
    }

    // A `ValueList` field is a `TAG_List` written verbatim, element type and
    // all — including the element type of an empty one, which no `Vec<T>` field
    // can express.
    if is_value_list(shape) {
        let list: &ValueList = peek.get::<ValueList>().map_err(reflect_err)?;
        return io::write_list::<F, W>(w, list, depth);
    }

    // `bstr::BString` field: write its raw bytes as a `String` tag payload.
    if is_bstring(shape) {
        let s: &bstr::BString = peek.get::<bstr::BString>().map_err(reflect_err)?;
        return io::write_str_payload::<F, W>(w, s.as_slice());
    }

    // Options: unwrap `Some` (a lone `None` should have been skipped upstream).
    if let Def::Option(_) = shape.def {
        return match unwrap_option(peek)? {
            Some(inner) => write_payload::<F, W>(w, inner, depth),
            None => Err(unsupported("cannot serialize a `None` value here")),
        };
    }

    if let Some(scalar) = ScalarType::try_from_shape(shape) {
        return write_scalar::<F, W>(w, peek, scalar);
    }

    if matches!(shape.def, Def::List(_) | Def::Array(_) | Def::Slice(_)) {
        return write_seq::<F, W>(w, peek, depth);
    }

    // Maps and structs → compounds.
    if let Def::Map(_) = shape.def {
        return write_map::<F, W>(w, peek, depth);
    }
    match shape.ty {
        Type::User(UserType::Struct(_)) => write_struct::<F, W>(w, peek, depth),
        Type::User(UserType::Enum(_)) => write_enum::<F, W>(w, peek),
        _ => Err(unsupported("serialization of this type is not supported")),
    }
}

fn write_scalar<F: EndiannessImpl, W: WriteBytesExt>(
    w: &mut W,
    peek: Peek,
    scalar: ScalarType,
) -> Result<(), Error> {
    match scalar {
        ScalarType::Bool => w.write_u8(u8::from(*peek.get::<bool>().map_err(reflect_err)?))?,
        ScalarType::I8 => w.write_i8(*peek.get::<i8>().map_err(reflect_err)?)?,
        // The `Byte` tag is a signed 8-bit integer; a `u8` is written by its bit
        // pattern, which is what `read_scalar`'s `cast_unsigned` reads back.
        ScalarType::U8 => w.write_u8(*peek.get::<u8>().map_err(reflect_err)?)?,
        ScalarType::I16 => io::write_i16::<F, W>(w, *peek.get::<i16>().map_err(reflect_err)?)?,
        ScalarType::I32 => io::write_i32::<F, W>(w, *peek.get::<i32>().map_err(reflect_err)?)?,
        ScalarType::I64 => io::write_i64::<F, W>(w, *peek.get::<i64>().map_err(reflect_err)?)?,
        ScalarType::F32 => io::write_f32::<F, W>(w, *peek.get::<f32>().map_err(reflect_err)?)?,
        ScalarType::F64 => io::write_f64::<F, W>(w, *peek.get::<f64>().map_err(reflect_err)?)?,
        ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
            let s = peek
                .as_str()
                .ok_or_else(|| unsupported("expected a string value"))?;
            io::write_str_payload::<F, W>(w, s.as_bytes())?;
        }
        _ => {
            return Err(unsupported(
                "serialization of this scalar type is not supported",
            ));
        }
    }
    Ok(())
}

fn write_seq<F: EndiannessImpl, W: WriteBytesExt>(
    w: &mut W,
    peek: Peek,
    depth: usize,
) -> Result<(), Error> {
    io::check_depth(depth)?;
    let shape = peek.shape();
    let elem_shape = match shape.def {
        Def::List(def) => def.t(),
        Def::Array(def) => def.t(),
        Def::Slice(def) => def.t(),
        _ => return Err(unsupported("expected a list, array or slice")),
    };
    let list = peek.into_list_like().map_err(reflect_err)?;
    let len = list.len();

    match list_tag(elem_shape) {
        FieldType::ByteArray => {
            io::write_seq_len::<F, W>(w, len)?;
            for item in list.iter() {
                w.write_u8(*item.get::<u8>().map_err(reflect_err)?)?;
            }
        }
        FieldType::IntArray => {
            io::write_seq_len::<F, W>(w, len)?;
            for item in list.iter() {
                io::write_i32::<F, W>(w, *item.get::<i32>().map_err(reflect_err)?)?;
            }
        }
        FieldType::LongArray => {
            io::write_seq_len::<F, W>(w, len)?;
            for item in list.iter() {
                io::write_i64::<F, W>(w, *item.get::<i64>().map_err(reflect_err)?)?;
            }
        }
        _ => {
            // A generic `List`: element type tag first, then length, then bodies.
            //
            // The element type is written *once* for the whole list, so an
            // element carrying a different tag would desync the stream — every
            // later element would be decoded against the declared type. When the
            // tag follows from the element *shape* (`static_tag`) the elements
            // cannot disagree and nothing needs checking; when it does not — a
            // `Vec<Value>`, a `Vec<Option<Value>>` — each element is checked and
            // a mixture is refused.
            let static_tag = tag_of_shape(elem_shape);
            let elem_tag = match list.iter().next() {
                Some(first) => tag_of(first)?,
                None => static_tag.unwrap_or(FieldType::End),
            };
            w.write_u8(elem_tag as u8)?;
            io::write_seq_len::<F, W>(w, len)?;
            for item in list.iter() {
                if static_tag.is_none() {
                    let tag = tag_of(item)?;
                    if tag != elem_tag {
                        return Err(Error::HeterogeneousList {
                            expected: elem_tag,
                            found: tag,
                        });
                    }
                }
                write_payload::<F, W>(w, item, depth + 1)?;
            }
        }
    }
    Ok(())
}

fn write_struct<F: EndiannessImpl, W: WriteBytesExt>(
    w: &mut W,
    peek: Peek,
    depth: usize,
) -> Result<(), Error> {
    io::check_depth(depth)?;
    let st = peek.into_struct().map_err(reflect_err)?;
    for (i, field) in st.ty().fields.iter().enumerate() {
        let raw = st.field(i).map_err(reflect_err)?;
        // Skip `None` optional fields entirely.
        let Some(value) = unwrap_option(raw)? else {
            continue;
        };
        let tag = tag_of(value)?;
        w.write_u8(tag as u8)?;
        io::write_str_payload::<F, W>(w, field.effective_name().as_bytes())?;
        write_payload::<F, W>(w, value, depth + 1)?;
    }
    w.write_u8(FieldType::End as u8)?;
    Ok(())
}

fn write_map<F: EndiannessImpl, W: WriteBytesExt>(
    w: &mut W,
    peek: Peek,
    depth: usize,
) -> Result<(), Error> {
    io::check_depth(depth)?;
    let map = peek.into_map().map_err(reflect_err)?;
    for (key, value) in map.iter() {
        let Some(value) = unwrap_option(value)? else {
            continue;
        };
        let tag = tag_of(value)?;
        w.write_u8(tag as u8)?;
        let key_str = key
            .as_str()
            .ok_or_else(|| unsupported("map keys must be strings"))?;
        io::write_str_payload::<F, W>(w, key_str.as_bytes())?;
        write_payload::<F, W>(w, value, depth + 1)?;
    }
    w.write_u8(FieldType::End as u8)?;
    Ok(())
}

/// Writes a unit enum variant, in whichever form its
/// `#[facet(nbtx::variant_as(...))]` declared: its (rename-aware) name as a
/// `String` payload, or its discriminant as a `Byte`/`Short`/`Int`/`Long` of the
/// declared width. The mode itself is resolved in [`crate::reflect`], shared
/// with the SNBT writer and the `Value` conversion.
fn write_enum<F: EndiannessImpl, W: WriteBytesExt>(w: &mut W, peek: Peek) -> Result<(), Error> {
    match enum_wire(peek)? {
        EnumWire::Name(name) => io::write_str_payload::<F, W>(w, name.as_bytes())?,
        EnumWire::Int(FieldType::Byte, v) => w.write_i8(v as i8)?,
        EnumWire::Int(FieldType::Short, v) => io::write_i16::<F, W>(w, v as i16)?,
        EnumWire::Int(FieldType::Int, v) => io::write_i32::<F, W>(w, v as i32)?,
        // `enum_wire` only ever answers with these four tags.
        EnumWire::Int(_, v) => io::write_i64::<F, W>(w, v)?,
    }
    Ok(())
}

/// Writes a complete NBT document (root tag byte, root name, payload).
///
/// # Root name
///
/// Always **empty**, for every `T`. Before 4.0 a struct root was written with
/// its Rust type name, which made the wire format depend on Rust identifiers and
/// broke interop with other Bedrock tooling. Wrap the value in a
/// [`Named<T>`](crate::Named) to write a real root name; that is the only case
/// in which a non-empty one is emitted.
fn write_root<F: EndiannessImpl, W: WriteBytesExt>(w: &mut W, peek: Peek) -> Result<(), Error> {
    if let Some(nr) = named::as_named_root(peek.shape()) {
        let st = peek.into_struct().map_err(reflect_err)?;
        let name = st.field(nr.name_idx).map_err(reflect_err)?;
        let name: &bstr::BString = name.get::<bstr::BString>().map_err(reflect_err)?;
        let value = st.field(nr.value_idx).map_err(reflect_err)?;
        let tag = tag_of(value)?;
        w.write_u8(tag as u8)?;
        io::write_str_payload::<F, W>(w, name.as_slice())?;
        return write_payload::<F, W>(w, value, 0);
    }

    let tag = tag_of(peek)?;
    w.write_u8(tag as u8)?;
    io::write_str_payload::<F, W>(w, b"")?;
    write_payload::<F, W>(w, peek, 0)
}

/// NBT data serializer.
#[derive(Debug)]
pub struct Serializer<W, E>
where
    W: WriteBytesExt,
    E: EndiannessImpl,
{
    writer: W,
    _marker: PhantomData<E>,
}

impl<W, E> Serializer<W, E>
where
    W: WriteBytesExt,
    E: EndiannessImpl,
{
    /// Creates a new serializer over the given writer.
    #[must_use]
    pub const fn new(w: W) -> Serializer<W, E> {
        Serializer {
            writer: w,
            _marker: PhantomData,
        }
    }

    /// Consumes the serializer and returns the inner writer.
    pub fn into_inner(self) -> W {
        self.writer
    }

    /// Serializes a value into the writer.
    pub fn serialize<'f, T: Facet<'f> + ?Sized>(&mut self, value: &'f T) -> Result<(), Error> {
        write_root::<E, W>(&mut self.writer, Peek::new(value))
    }
}

/// Serializes `value` in the given endian format, returning a new buffer.
pub fn to_bytes<'f, E: EndiannessImpl>(
    value: &'f (impl Facet<'f> + ?Sized),
) -> Result<Vec<u8>, Error> {
    let mut buf = Vec::new();
    write_root::<E, _>(&mut buf, Peek::new(value))?;
    Ok(buf)
}

/// Serializes `value` in the given endian format into an existing writer.
pub fn to_bytes_in<'f, E: EndiannessImpl>(
    writer: &mut impl WriteBytesExt,
    value: &'f (impl Facet<'f> + ?Sized),
) -> Result<(), Error> {
    write_root::<E, _>(writer, Peek::new(value))
}

macro_rules! endian_fns {
    ($to:ident, $to_in:ident, $endian:ty, $doc:literal) => {
        #[doc = $doc]
        pub fn $to<'f>(value: &'f (impl Facet<'f> + ?Sized)) -> Result<Vec<u8>, Error> {
            to_bytes::<$endian>(value)
        }

        #[doc = $doc]
        pub fn $to_in<'f>(
            writer: &mut impl WriteBytesExt,
            value: &'f (impl Facet<'f> + ?Sized),
        ) -> Result<(), Error> {
            to_bytes_in::<$endian>(writer, value)
        }
    };
}

endian_fns!(
    to_be_bytes,
    to_be_bytes_in,
    BigEndian,
    "Serializes `value` in big-endian format (fixed-width big-endian integers)."
);
endian_fns!(
    to_le_bytes,
    to_le_bytes_in,
    LittleEndian,
    "Serializes `value` in little-endian format (Minecraft: Bedrock Edition, disk)."
);
endian_fns!(
    to_varint_bytes,
    to_varint_bytes_in,
    VarintEndian,
    "Serializes `value` in varint little-endian format (Minecraft: Bedrock Edition, network)."
);
