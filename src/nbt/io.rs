//! Low-level, endianness-aware read/write primitives shared by the NBT
//! serializer and deserializer, plus the byte-exact codec for the dynamic
//! [`Value`] type.
//!
//! All length/integer encodings match the three NBT variants byte-for-byte:
//! * [`Variant::BigEndian`] — fixed-width big-endian integers.
//! * [`Variant::LittleEndian`] — fixed-width little-endian integers.
//! * [`Variant::VarintEndian`] — little-endian shorts/floats, but varints for
//!   ints, longs and every length prefix.

use std::io::Read;

use crate::value::Compound;
use byteorder::{BigEndian, LittleEndian, ReadBytesExt, WriteBytesExt};
use integer_encoding::{VarIntReader, VarIntWriter};

use crate::error::{InvalidVarint, StringTooLong, UnexpectedEnd};
use crate::{EndiannessImpl, Error, FieldType, MAX_STRING_LEN, Value, ValueList, Variant};

/// Upper bound on how much capacity we speculatively reserve from an
/// attacker-controlled wire length before any bytes are actually read. The real
/// allocation still grows to fit genuine data; this only caps the *initial*
/// reservation so a bogus multi-gigabyte length can't OOM the process up front.
const MAX_PREALLOC: usize = 4096;

/// Maximum number of bytes a 32-bit varint may occupy on the wire.
///
/// `ceil(32 / 7) == 5`: seven payload bits per byte. A longer varint would
/// shift past the width of an `i32`. Used only to label the error; the bound
/// itself is enforced by `integer_encoding`.
const MAX_VARINT32_BYTES: usize = 5;

/// Maximum number of bytes a 64-bit varint may occupy on the wire.
///
/// `ceil(64 / 7) == 10`; see [`MAX_VARINT32_BYTES`].
const MAX_VARINT64_BYTES: usize = 10;

// The depth guard itself (`check_depth`/`max_depth_exceeded`) lives in the crate
// root: the SNBT codec enforces the same `MAX_DEPTH` and must build without the
// `nbt` feature.
pub(crate) use crate::check_depth;

fn invalid_varint(max_bytes: usize) -> Error {
    Error::InvalidVarint(InvalidVarint {
        max_bytes,
        #[cfg(feature = "error-context")]
        at: String::from("unknown"),
        #[cfg(feature = "error-context")]
        index: None,
    })
}

fn string_too_long(len: usize) -> Error {
    Error::StringTooLong(StringTooLong {
        len,
        max: MAX_STRING_LEN,
        #[cfg(feature = "error-context")]
        at: String::from("unknown"),
        #[cfg(feature = "error-context")]
        index: None,
    })
}

/// Translates an `integer_encoding` read failure into an [`Error`].
///
/// The crate signals an overlong varint (one that runs past its type's maximum
/// byte count) with [`std::io::ErrorKind::InvalidData`]; a genuinely truncated
/// stream comes back as [`std::io::ErrorKind::UnexpectedEof`] and keeps flowing
/// through the normal `From<io::Error>` mapping.
fn varint_err(e: std::io::Error, max_bytes: usize) -> Error {
    if e.kind() == std::io::ErrorKind::InvalidData {
        invalid_varint(max_bytes)
    } else {
        Error::from(e)
    }
}

/// Reads an unsigned LEB128 varint, bounded to [`MAX_VARINT32_BYTES`].
fn read_varu32<R: ReadBytesExt>(r: &mut R) -> Result<u32, Error> {
    r.read_varint::<u32>()
        .map_err(|e| varint_err(e, MAX_VARINT32_BYTES))
}

/// Reads a zigzag-encoded 32-bit varint.
fn read_vari32<R: ReadBytesExt>(r: &mut R) -> Result<i32, Error> {
    r.read_varint::<i32>()
        .map_err(|e| varint_err(e, MAX_VARINT32_BYTES))
}

/// Reads a zigzag-encoded 64-bit varint.
fn read_vari64<R: ReadBytesExt>(r: &mut R) -> Result<i64, Error> {
    r.read_varint::<i64>()
        .map_err(|e| varint_err(e, MAX_VARINT64_BYTES))
}

pub(crate) fn write_i16<F: EndiannessImpl, W: WriteBytesExt>(
    w: &mut W,
    v: i16,
) -> Result<(), Error> {
    match F::AS_ENUM {
        Variant::BigEndian => w.write_i16::<BigEndian>(v)?,
        Variant::LittleEndian | Variant::VarintEndian => w.write_i16::<LittleEndian>(v)?,
    }
    Ok(())
}

pub(crate) fn write_i32<F: EndiannessImpl, W: WriteBytesExt>(
    w: &mut W,
    v: i32,
) -> Result<(), Error> {
    match F::AS_ENUM {
        Variant::BigEndian => w.write_i32::<BigEndian>(v)?,
        Variant::LittleEndian => w.write_i32::<LittleEndian>(v)?,
        Variant::VarintEndian => {
            w.write_varint(v)?;
        }
    }
    Ok(())
}

pub(crate) fn write_i64<F: EndiannessImpl, W: WriteBytesExt>(
    w: &mut W,
    v: i64,
) -> Result<(), Error> {
    match F::AS_ENUM {
        Variant::BigEndian => w.write_i64::<BigEndian>(v)?,
        Variant::LittleEndian => w.write_i64::<LittleEndian>(v)?,
        Variant::VarintEndian => {
            w.write_varint(v)?;
        }
    }
    Ok(())
}

pub(crate) fn write_f32<F: EndiannessImpl, W: WriteBytesExt>(
    w: &mut W,
    v: f32,
) -> Result<(), Error> {
    match F::AS_ENUM {
        Variant::BigEndian => w.write_f32::<BigEndian>(v)?,
        Variant::LittleEndian | Variant::VarintEndian => w.write_f32::<LittleEndian>(v)?,
    }
    Ok(())
}

pub(crate) fn write_f64<F: EndiannessImpl, W: WriteBytesExt>(
    w: &mut W,
    v: f64,
) -> Result<(), Error> {
    match F::AS_ENUM {
        Variant::BigEndian => w.write_f64::<BigEndian>(v)?,
        Variant::LittleEndian | Variant::VarintEndian => w.write_f64::<LittleEndian>(v)?,
    }
    Ok(())
}

/// Writes a `String`-tag length prefix: a `u16` for the big/little variants, a
/// `u32` varint for the varint variant.
///
/// Rejects anything over [`MAX_STRING_LEN`]. Without the check the big/little
/// variants would wrap the length to a `u16` and silently emit a stream that
/// decodes as something else.
pub(crate) fn write_str_len<F: EndiannessImpl, W: WriteBytesExt>(
    w: &mut W,
    len: usize,
) -> Result<(), Error> {
    if len > MAX_STRING_LEN {
        return Err(string_too_long(len));
    }
    match F::AS_ENUM {
        Variant::BigEndian => w.write_u16::<BigEndian>(len as u16)?,
        Variant::LittleEndian => w.write_u16::<LittleEndian>(len as u16)?,
        Variant::VarintEndian => {
            w.write_varint(len as u32)?;
        }
    }
    Ok(())
}

/// Writes a `List`/array length prefix: a signed `i32` for the big/little
/// variants, an `i32` varint for the varint variant.
pub(crate) fn write_seq_len<F: EndiannessImpl, W: WriteBytesExt>(
    w: &mut W,
    len: usize,
) -> Result<(), Error> {
    match F::AS_ENUM {
        Variant::BigEndian => w.write_i32::<BigEndian>(len as i32)?,
        Variant::LittleEndian => w.write_i32::<LittleEndian>(len as i32)?,
        Variant::VarintEndian => {
            w.write_varint(len as i32)?;
        }
    }
    Ok(())
}

/// Writes a raw `String`-tag payload (length prefix + raw bytes, no UTF-8
/// validation).
pub(crate) fn write_str_payload<F: EndiannessImpl, W: WriteBytesExt>(
    w: &mut W,
    bytes: &[u8],
) -> Result<(), Error> {
    write_str_len::<F, W>(w, bytes.len())?;
    w.write_all(bytes)?;
    Ok(())
}

pub(crate) fn read_i8<R: ReadBytesExt>(r: &mut R) -> Result<i8, Error> {
    Ok(r.read_i8()?)
}

pub(crate) fn read_i16<F: EndiannessImpl, R: ReadBytesExt>(r: &mut R) -> Result<i16, Error> {
    Ok(match F::AS_ENUM {
        Variant::BigEndian => r.read_i16::<BigEndian>()?,
        Variant::LittleEndian | Variant::VarintEndian => r.read_i16::<LittleEndian>()?,
    })
}

pub(crate) fn read_i32<F: EndiannessImpl, R: ReadBytesExt>(r: &mut R) -> Result<i32, Error> {
    Ok(match F::AS_ENUM {
        Variant::BigEndian => r.read_i32::<BigEndian>()?,
        Variant::LittleEndian => r.read_i32::<LittleEndian>()?,
        Variant::VarintEndian => read_vari32(r)?,
    })
}

pub(crate) fn read_i64<F: EndiannessImpl, R: ReadBytesExt>(r: &mut R) -> Result<i64, Error> {
    Ok(match F::AS_ENUM {
        Variant::BigEndian => r.read_i64::<BigEndian>()?,
        Variant::LittleEndian => r.read_i64::<LittleEndian>()?,
        Variant::VarintEndian => read_vari64(r)?,
    })
}

pub(crate) fn read_f32<F: EndiannessImpl, R: ReadBytesExt>(r: &mut R) -> Result<f32, Error> {
    Ok(match F::AS_ENUM {
        Variant::BigEndian => r.read_f32::<BigEndian>()?,
        Variant::LittleEndian | Variant::VarintEndian => r.read_f32::<LittleEndian>()?,
    })
}

pub(crate) fn read_f64<F: EndiannessImpl, R: ReadBytesExt>(r: &mut R) -> Result<f64, Error> {
    Ok(match F::AS_ENUM {
        Variant::BigEndian => r.read_f64::<BigEndian>()?,
        Variant::LittleEndian | Variant::VarintEndian => r.read_f64::<LittleEndian>()?,
    })
}

/// Reads a `String`-tag length prefix.
///
/// The big/little variants are naturally bounded by their `u16` prefix. The
/// varint variant is not, so its length is rejected before any allocation once
/// it exceeds [`MAX_STRING_LEN`]: three bytes of input must not be able to make
/// the decoder commit to a large read.
pub(crate) fn read_str_len<F: EndiannessImpl, R: ReadBytesExt>(r: &mut R) -> Result<u32, Error> {
    Ok(match F::AS_ENUM {
        Variant::BigEndian => u32::from(r.read_u16::<BigEndian>()?),
        Variant::LittleEndian => u32::from(r.read_u16::<LittleEndian>()?),
        Variant::VarintEndian => {
            let len = read_varu32(r)?;
            if len as usize > MAX_STRING_LEN {
                return Err(string_too_long(len as usize));
            }
            len
        }
    })
}

/// Reads a `List`/array length prefix (as an unsigned count).
pub(crate) fn read_seq_len<F: EndiannessImpl, R: ReadBytesExt>(r: &mut R) -> Result<u32, Error> {
    Ok(match F::AS_ENUM {
        Variant::BigEndian => r.read_i32::<BigEndian>()?.cast_unsigned(),
        Variant::LittleEndian => r.read_i32::<LittleEndian>()?.cast_unsigned(),
        Variant::VarintEndian => read_vari32(r)?.cast_unsigned(),
    })
}

/// Reads exactly `len` bytes without trusting `len` for the initial allocation.
///
/// `len` comes straight off the wire, so we cap the up-front reservation at
/// [`MAX_PREALLOC`] and let [`Read::read_to_end`] grow the buffer to fit the
/// bytes that are actually present. A truncated stream therefore yields fewer
/// bytes than promised and surfaces as an [`Error::UnexpectedEof`].
fn read_exact_bounded<R: ReadBytesExt>(r: &mut R, len: usize) -> Result<Vec<u8>, Error> {
    let mut buf = Vec::with_capacity(len.min(MAX_PREALLOC));
    let read = r.take(len as u64).read_to_end(&mut buf)?;
    if read != len {
        return Err(Error::from(std::io::Error::from(
            std::io::ErrorKind::UnexpectedEof,
        )));
    }
    Ok(buf)
}

/// Reads a raw `String`-tag payload (length prefix + raw bytes).
pub(crate) fn read_str_payload<F: EndiannessImpl, R: ReadBytesExt>(
    r: &mut R,
) -> Result<Vec<u8>, Error> {
    let len = read_str_len::<F, R>(r)? as usize;
    read_exact_bounded(r, len)
}

/// Reads a single tag byte and converts it to a [`FieldType`].
pub(crate) fn read_tag<R: ReadBytesExt>(r: &mut R) -> Result<FieldType, Error> {
    FieldType::try_from(
        r.read_u8()?,
        #[cfg(feature = "error-context")]
        &mut None,
        #[cfg(feature = "error-context")]
        None,
    )
}

/// Builds the "a `TAG_End` turned up where a value was due" error.
fn unexpected_end() -> Error {
    Error::UnexpectedEnd(UnexpectedEnd {
        #[cfg(feature = "error-context")]
        at: String::from("unknown"),
        #[cfg(feature = "error-context")]
        index: None,
    })
}

/// Writes the *payload* of a [`Value`] (its tag byte has already been written by
/// the caller).
///
/// `depth` is the number of containers already entered; it is checked against
/// [`MAX_DEPTH`] before recursing so that a pathologically nested `Value` fails
/// with an error instead of overflowing the stack.
pub(crate) fn write_value<F: EndiannessImpl, W: WriteBytesExt>(
    w: &mut W,
    value: &Value,
    depth: usize,
) -> Result<(), Error> {
    // Only the two recursive variants are handled here; everything else goes to
    // `write_leaf_value`. See `read_value` for why the recursive frame is kept
    // deliberately small.
    match value {
        Value::List(list) => write_list::<F, W>(w, list, depth),
        Value::Compound(map) => write_compound::<F, W>(w, map, depth),
        leaf => write_leaf_value::<F, W>(w, leaf),
    }
}

/// Writes a `TAG_List` payload: the element-type byte, the length, then the
/// element payloads with no per-element tag byte.
///
/// The list is already typed, so there is nothing to validate and no
/// `Value` to build per element — the payloads are written straight out of the
/// typed vectors. `depth` counts the containers already entered and is checked
/// before the elements are written, so one `List` costs exactly one level,
/// unchanged from before `ValueList` existed.
pub(crate) fn write_list<F: EndiannessImpl, W: WriteBytesExt>(
    w: &mut W,
    list: &ValueList,
    depth: usize,
) -> Result<(), Error> {
    check_depth(depth)?;
    w.write_u8(list.element_type() as u8)?;
    write_seq_len::<F, W>(w, list.len())?;
    // Only the two recursive element types live in this frame; the ten leaf
    // ones go to an `#[inline(never)]` helper, for the reason `read_value`
    // spells out.
    match list {
        ValueList::List(items) => {
            for item in items {
                write_list::<F, W>(w, item, depth + 1)?;
            }
        }
        ValueList::Compound(items) => {
            for item in items {
                write_compound::<F, W>(w, item, depth + 1)?;
            }
        }
        leaf => write_leaf_list::<F, W>(w, leaf)?,
    }
    Ok(())
}

/// Writes a compound *body*: `<tag><key><payload>` per entry, then a `TAG_End`.
fn write_compound<F: EndiannessImpl, W: WriteBytesExt>(
    w: &mut W,
    map: &Compound,
    depth: usize,
) -> Result<(), Error> {
    check_depth(depth)?;
    for (k, v) in map {
        w.write_u8(v.discriminant())?;
        write_str_payload::<F, W>(w, k.as_slice())?;
        // Dispatch to the nested container directly instead of bouncing through
        // `write_value`, so a deep document costs one frame per level.
        match v {
            Value::List(list) => write_list::<F, W>(w, list, depth + 1)?,
            Value::Compound(inner) => write_compound::<F, W>(w, inner, depth + 1)?,
            leaf => write_leaf_value::<F, W>(w, leaf)?,
        }
    }
    w.write_u8(FieldType::End as u8)?;
    Ok(())
}

/// Writes the elements of a list whose element type is not itself a container.
/// Split out of [`write_list`] (and never inlined) to keep that recursive frame
/// small.
#[inline(never)]
fn write_leaf_list<F: EndiannessImpl, W: WriteBytesExt>(
    w: &mut W,
    list: &ValueList,
) -> Result<(), Error> {
    match list {
        // The empty list: the element-type byte and the zero length are the
        // whole encoding.
        ValueList::End => {}
        ValueList::Byte(items) => {
            for &v in items {
                w.write_i8(v)?;
            }
        }
        ValueList::Short(items) => {
            for &v in items {
                write_i16::<F, W>(w, v)?;
            }
        }
        ValueList::Int(items) => {
            for &v in items {
                write_i32::<F, W>(w, v)?;
            }
        }
        ValueList::Long(items) => {
            for &v in items {
                write_i64::<F, W>(w, v)?;
            }
        }
        ValueList::Float(items) => {
            for &v in items {
                write_f32::<F, W>(w, v)?;
            }
        }
        ValueList::Double(items) => {
            for &v in items {
                write_f64::<F, W>(w, v)?;
            }
        }
        ValueList::ByteArray(items) => {
            for bytes in items {
                write_seq_len::<F, W>(w, bytes.len())?;
                w.write_all(bytes)?;
            }
        }
        ValueList::String(items) => {
            for s in items {
                write_str_payload::<F, W>(w, s.as_slice())?;
            }
        }
        ValueList::IntArray(items) => {
            for ints in items {
                write_seq_len::<F, W>(w, ints.len())?;
                for &i in ints {
                    write_i32::<F, W>(w, i)?;
                }
            }
        }
        ValueList::LongArray(items) => {
            for longs in items {
                write_seq_len::<F, W>(w, longs.len())?;
                for &l in longs {
                    write_i64::<F, W>(w, l)?;
                }
            }
        }
        // Unreachable: `write_list` handles the two recursive element types.
        ValueList::List(_) | ValueList::Compound(_) => {
            return Err(Error::Other(String::from(
                "internal error: recursive list routed to the leaf writer",
            )));
        }
    }
    Ok(())
}

/// Writes the payload of a non-recursive [`Value`] variant. Split out of
/// [`write_value`] (and never inlined) to keep that recursive frame small.
#[inline(never)]
fn write_leaf_value<F: EndiannessImpl, W: WriteBytesExt>(
    w: &mut W,
    value: &Value,
) -> Result<(), Error> {
    match value {
        Value::Byte(v) => w.write_i8(*v)?,
        Value::Short(v) => write_i16::<F, W>(w, *v)?,
        Value::Int(v) => write_i32::<F, W>(w, *v)?,
        Value::Long(v) => write_i64::<F, W>(w, *v)?,
        Value::Float(v) => write_f32::<F, W>(w, *v)?,
        Value::Double(v) => write_f64::<F, W>(w, *v)?,
        Value::ByteArray(bytes) => {
            write_seq_len::<F, W>(w, bytes.len())?;
            w.write_all(bytes)?;
        }
        Value::String(s) => write_str_payload::<F, W>(w, s.as_slice())?,
        Value::IntArray(ints) => {
            write_seq_len::<F, W>(w, ints.len())?;
            for &i in ints {
                write_i32::<F, W>(w, i)?;
            }
        }
        Value::LongArray(longs) => {
            write_seq_len::<F, W>(w, longs.len())?;
            for &l in longs {
                write_i64::<F, W>(w, l)?;
            }
        }
        // Unreachable: `write_value` handles the two recursive variants itself.
        Value::List(_) | Value::Compound(_) => {
            return Err(Error::Other(String::from(
                "internal error: recursive value routed to the leaf writer",
            )));
        }
    }
    Ok(())
}

/// Reads the *payload* of a [`Value`] whose tag byte (`ty`) has already been
/// consumed by the caller.
///
/// `depth` is the number of containers already entered; it is checked against
/// [`MAX_DEPTH`] before recursing, so a malicious stream of nested `List`s or
/// `Compound`s yields [`Error::MaxDepthExceeded`] instead of overflowing the
/// stack (which would abort the process).
pub(crate) fn read_value<F: EndiannessImpl, R: ReadBytesExt>(
    r: &mut R,
    ty: FieldType,
    depth: usize,
) -> Result<Value, Error> {
    // Only the two recursive arms live in this frame; everything else is
    // delegated to `read_leaf_value`. Keeping the frame small matters: this
    // function is one of the two frames per nesting level, and an unoptimised
    // build would otherwise exhaust a default 2 MiB thread stack well before
    // reaching `MAX_DEPTH`.
    Ok(match ty {
        FieldType::List => Value::List(read_list::<F, R>(r, depth)?),
        FieldType::Compound => Value::Compound(read_compound::<F, R>(r, depth)?),
        leaf => read_leaf_value::<F, R>(r, leaf)?,
    })
}

/// Reads a `TAG_List` body — element-type byte, length, then that many payloads
/// of that one type — into a typed [`ValueList`].
///
/// # `TAG_End` element type
///
/// Legal at length 0 only, and that is [`ValueList::End`]: the empty list that
/// names no element type. A *non-empty* `End` list has no payloads to read and
/// is rejected as [`Error::UnexpectedEnd`], matching pmmp/NBT ("Unexpected
/// non-empty list of `TAG_End`") and gophertunnel's `UnexpectedTagError`.
///
/// `depth` is checked once, before any element is read, so one `List` costs
/// exactly one level whatever it holds.
pub(crate) fn read_list<F: EndiannessImpl, R: ReadBytesExt>(
    r: &mut R,
    depth: usize,
) -> Result<ValueList, Error> {
    check_depth(depth)?;
    let elem = read_tag(r)?;
    let len = read_seq_len::<F, R>(r)? as usize;
    // Only the two recursive element types live in this frame; see `read_value`.
    Ok(match elem {
        FieldType::End => {
            if len != 0 {
                return Err(unexpected_end());
            }
            ValueList::End
        }
        FieldType::List => {
            let mut out = Vec::with_capacity(len.min(MAX_PREALLOC));
            for _ in 0..len {
                out.push(read_list::<F, R>(r, depth + 1)?);
            }
            ValueList::List(out)
        }
        FieldType::Compound => {
            let mut out = Vec::with_capacity(len.min(MAX_PREALLOC));
            for _ in 0..len {
                out.push(read_compound::<F, R>(r, depth + 1)?);
            }
            ValueList::Compound(out)
        }
        leaf => read_leaf_list::<F, R>(r, leaf, len)?,
    })
}

/// Reads the elements of a list whose element type is not itself a container.
/// Split out of [`read_list`] (and never inlined) so that recursive frame stays
/// small; every loop caps its up-front reservation at [`MAX_PREALLOC`], since
/// `len` came straight off the wire.
#[inline(never)]
fn read_leaf_list<F: EndiannessImpl, R: ReadBytesExt>(
    r: &mut R,
    elem: FieldType,
    len: usize,
) -> Result<ValueList, Error> {
    /// Reads `len` elements with `$read`, into the `ValueList::$variant` they
    /// belong to.
    macro_rules! collect {
        ($variant:ident, $read:expr) => {{
            let mut out = Vec::with_capacity(len.min(MAX_PREALLOC));
            for _ in 0..len {
                out.push($read?);
            }
            ValueList::$variant(out)
        }};
    }
    Ok(match elem {
        FieldType::Byte => collect!(Byte, read_i8(r)),
        FieldType::Short => collect!(Short, read_i16::<F, R>(r)),
        FieldType::Int => collect!(Int, read_i32::<F, R>(r)),
        FieldType::Long => collect!(Long, read_i64::<F, R>(r)),
        FieldType::Float => collect!(Float, read_f32::<F, R>(r)),
        FieldType::Double => collect!(Double, read_f64::<F, R>(r)),
        FieldType::ByteArray => collect!(ByteArray, read_byte_array::<F, R>(r)),
        FieldType::String => collect!(String, read_str_payload::<F, R>(r).map(bstr::BString::from)),
        FieldType::IntArray => collect!(IntArray, read_int_array::<F, R>(r)),
        FieldType::LongArray => collect!(LongArray, read_long_array::<F, R>(r)),
        // Unreachable: `read_list` handles `End` and the two recursive tags.
        FieldType::End | FieldType::List | FieldType::Compound => {
            return Err(Error::Other(String::from(
                "internal error: recursive tag routed to the leaf list reader",
            )));
        }
    })
}

/// Reads a `ByteArray` payload (length prefix + that many bytes).
fn read_byte_array<F: EndiannessImpl, R: ReadBytesExt>(r: &mut R) -> Result<Vec<u8>, Error> {
    let len = read_seq_len::<F, R>(r)? as usize;
    read_exact_bounded(r, len)
}

/// Reads an `IntArray` payload (length prefix + that many ints).
fn read_int_array<F: EndiannessImpl, R: ReadBytesExt>(r: &mut R) -> Result<Vec<i32>, Error> {
    let len = read_seq_len::<F, R>(r)? as usize;
    let mut out = Vec::with_capacity(len.min(MAX_PREALLOC));
    for _ in 0..len {
        out.push(read_i32::<F, R>(r)?);
    }
    Ok(out)
}

/// Reads a `LongArray` payload (length prefix + that many longs).
fn read_long_array<F: EndiannessImpl, R: ReadBytesExt>(r: &mut R) -> Result<Vec<i64>, Error> {
    let len = read_seq_len::<F, R>(r)? as usize;
    let mut out = Vec::with_capacity(len.min(MAX_PREALLOC));
    for _ in 0..len {
        out.push(read_i64::<F, R>(r)?);
    }
    Ok(out)
}

/// Reads the payload of a non-recursive tag. Split out of [`read_value`] so the
/// recursive frame stays small; `#[inline(never)]` keeps the split effective in
/// unoptimised builds, where it matters most.
#[inline(never)]
fn read_leaf_value<F: EndiannessImpl, R: ReadBytesExt>(
    r: &mut R,
    ty: FieldType,
) -> Result<Value, Error> {
    Ok(match ty {
        FieldType::End => return Err(unexpected_end()),
        FieldType::Byte => Value::Byte(read_i8(r)?),
        FieldType::Short => Value::Short(read_i16::<F, R>(r)?),
        FieldType::Int => Value::Int(read_i32::<F, R>(r)?),
        FieldType::Long => Value::Long(read_i64::<F, R>(r)?),
        FieldType::Float => Value::Float(read_f32::<F, R>(r)?),
        FieldType::Double => Value::Double(read_f64::<F, R>(r)?),
        FieldType::ByteArray => Value::ByteArray(read_byte_array::<F, R>(r)?),
        FieldType::String => Value::String(bstr::BString::from(read_str_payload::<F, R>(r)?)),
        FieldType::IntArray => Value::IntArray(read_int_array::<F, R>(r)?),
        FieldType::LongArray => Value::LongArray(read_long_array::<F, R>(r)?),
        // Unreachable: `read_value` handles the two recursive tags itself.
        FieldType::List | FieldType::Compound => {
            return Err(Error::Other(String::from(
                "internal error: recursive tag routed to the leaf reader",
            )));
        }
    })
}

/// Reads a compound *body* (a sequence of `<tag><key><value>` entries ending in
/// a `TAG_End`) into a [`Compound`], preserving on-disk key order under the
/// default `preserve_order` feature.
///
/// # Duplicate keys
///
/// A key that has already been seen is **dropped**: the first occurrence wins
/// and keeps its original position. Duplicate keys are a known corruption
/// pattern in old world saves, and the first value is the one the rest of the
/// document was written against, so first-wins is the recovery that preserves
/// the document's internal consistency. (The naive implementation — inserting
/// into a map — gives last-wins instead.) The dropped value is still fully
/// parsed, so the stream stays in sync.
pub(crate) fn read_compound<F: EndiannessImpl, R: ReadBytesExt>(
    r: &mut R,
    depth: usize,
) -> Result<Compound, Error> {
    check_depth(depth)?;
    let mut map = Compound::new();
    loop {
        let ty = read_tag(r)?;
        if ty == FieldType::End {
            break;
        }
        let key = bstr::BString::from(read_str_payload::<F, R>(r)?);
        // Recurse straight into `read_compound` for a nested compound instead of
        // bouncing through `read_value`. A chain of nested compounds is the
        // cheapest deep document an attacker can send, and this halves the
        // number of stack frames it costs (one per level instead of two).
        let value = match ty {
            FieldType::Compound => Value::Compound(read_compound::<F, R>(r, depth + 1)?),
            FieldType::List => Value::List(read_list::<F, R>(r, depth + 1)?),
            leaf => read_leaf_value::<F, R>(r, leaf)?,
        };
        // First occurrence wins. `entry`/`or_insert` behaves identically on both
        // `Compound` backings (`IndexMap` under `preserve_order`, `BTreeMap`
        // otherwise) and keeps the first entry's position.
        map.entry(key).or_insert(value);
    }
    Ok(map)
}
