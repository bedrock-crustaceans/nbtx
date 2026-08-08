//! NBT (binary) deserialization, driven by `facet` reflection.
//!
//! The deserializer reads NBT bytes and builds a [`facet_reflect::Partial`] for
//! the target type. The NBT tag drives how bytes are read; the *target* Rust
//! type drives how they are stored:
//!
//! * A dynamic [`Value`] target captures the exact tag of every node.
//! * A `#[derive(Facet)]` struct matches NBT keys against field names
//!   (`#[facet(rename = "...")]`-aware); an unknown key is an error unless the
//!   struct opts out with `#[facet(nbtx::allow_unknown_fields)]`, and missing
//!   `Option` fields default to `None`.
//! * `Vec<T>`/`[T; N]` read `List`, `ByteArray`, `IntArray` or `LongArray`
//!   tags; a map reads a `Compound`.
//! * An enum reads the tag its mandatory `#[facet(nbtx::variant_as(<mode>))]`
//!   declares — a `String` naming the variant (`#[facet(rename = "...")]`-aware)
//!   or a fixed-width integer holding its discriminant — and errors if the enum
//!   declared no mode, if the tag is a different one, or if no variant claims
//!   the number that arrived.
//!
//! A field (or an enum's discriminant) may accept *more* than its own tag by
//! naming the others in `#[facet(nbtx::lenient_width(<types>))]`; each is then
//! converted into the declared type losslessly or reported. That is a decode-only
//! tolerance — `nbt::ser` never consults it — and the rules live in
//! [`crate::reflect`], shared with the other two codecs. See
//! [`lenient_width`](crate::Attr::LenientWidth).

use byteorder::ReadBytesExt;
use facet::Facet;
use facet_core::{Def, ScalarType, Shape, Type, UserType};
use facet_reflect::Partial;

use crate::error::UnexpectedEnd;
use crate::named;
use crate::nbt::io;
// Shared with the SNBT codec and the `Value` conversion; see `crate::reflect`.
use crate::reflect::{
    Lenient, VariantAs, WireScalar, is_bstring, is_value, lenient_discriminant, reflect_err,
    scalar_tag, set_lenient, unexpected_type, unknown_field, unsupported,
};
use crate::{BigEndian, EndiannessImpl, Error, FieldType, LittleEndian, VarintEndian};

type Part<'f> = Partial<'f, true>;

/// Reads the payload of NBT tag `tag` into the current partial frame, whose
/// target type is described by `shape`.
///
/// `depth` is the number of containers already entered; every nested-container
/// reader checks it against [`MAX_DEPTH`](crate::MAX_DEPTH) before recursing, so
/// a maliciously deep document errors out instead of overflowing the stack.
///
/// `lenient` is the `#[facet(nbtx::lenient_width(...))]` declaration of the
/// struct field this subtree belongs to, carried down so that one declaration
/// widens every element of a `Vec` and the inside of an `Option` alike. It is
/// one `Copy` byte precisely because it rides this recursive path.
fn read_into<'f, F: EndiannessImpl, R: ReadBytesExt>(
    p: Part<'f>,
    shape: &'static Shape,
    tag: FieldType,
    r: &mut R,
    depth: usize,
    lenient: Lenient,
) -> Result<Part<'f>, Error> {
    // This function is on the recursion path, so it deliberately keeps almost
    // nothing in its own frame: every non-recursive case is delegated to an
    // `#[inline(never)]` leaf helper. Unoptimised builds give each temporary in
    // a function its own stack slot without reuse, so folding the leaf cases
    // back in here would multiply the per-level stack cost and let a document
    // well inside `MAX_DEPTH` still overflow the stack.

    // A dynamic `Value` target swallows the whole (possibly nested) subtree.
    if is_value(shape) {
        return read_value_leaf::<F, R>(p, tag, r, depth);
    }

    // Option: the key was present, so this is `Some`.
    if let Def::Option(def) = shape.def {
        let p = p.begin_some().map_err(reflect_err)?;
        let p = read_into::<F, R>(p, def.t(), tag, r, depth, lenient)?;
        return p.end().map_err(reflect_err);
    }

    // `BString` reflects as a `Def::List<u8>` and scalars can carry a `Def` of
    // their own, so both are ruled out before the container dispatch below.
    if !is_bstring(shape) && ScalarType::try_from_shape(shape).is_none() {
        match shape.def {
            Def::List(def) => return read_seq::<F, R>(p, def.t(), tag, r, None, depth, lenient),
            Def::Array(def) => {
                return read_seq::<F, R>(p, def.t(), tag, r, Some(def.n), depth, lenient);
            }
            Def::Map(def) => return read_map::<F, R>(p, def.k(), def.v(), tag, r, depth),
            _ => {}
        }
        if let Type::User(UserType::Struct(_)) = shape.ty {
            return read_struct::<F, R>(p, shape, tag, r, depth);
        }
    }

    read_leaf::<F, R>(p, shape, tag, r, lenient)
}

/// Reads a dynamic [`Value`] subtree. Split out of [`read_into`] so the 80-byte
/// `Value` temporary does not sit in every recursive frame.
#[inline(never)]
fn read_value_leaf<'f, F: EndiannessImpl, R: ReadBytesExt>(
    p: Part<'f>,
    tag: FieldType,
    r: &mut R,
    depth: usize,
) -> Result<Part<'f>, Error> {
    let v = io::read_value::<F, R>(r, tag, depth)?;
    p.set(v).map_err(reflect_err)
}

/// All the non-recursive target types. Split out of [`read_into`] to keep the
/// recursive frame small; see the comment there.
#[inline(never)]
fn read_leaf<'f, F: EndiannessImpl, R: ReadBytesExt>(
    p: Part<'f>,
    shape: &'static Shape,
    tag: FieldType,
    r: &mut R,
    lenient: Lenient,
) -> Result<Part<'f>, Error> {
    // `bstr::BString` field: read a `String` tag's raw bytes without UTF-8
    // validation (unlike a concrete `String`, which must be valid UTF-8).
    if is_bstring(shape) {
        if tag != FieldType::String {
            return Err(unexpected_type(FieldType::String, tag));
        }
        let bytes = io::read_str_payload::<F, R>(r)?;
        return p.set(bstr::BString::from(bytes)).map_err(reflect_err);
    }

    if let Some(scalar) = ScalarType::try_from_shape(shape) {
        return read_scalar::<F, R>(p, scalar, tag, r, lenient);
    }

    match shape.ty {
        // A unit enum variant arrives in whichever form the enum's mandatory
        // `#[facet(nbtx::variant_as(...))]` declared — its name as a String tag,
        // or its discriminant as a fixed-width scalar (see `write_enum` in
        // `nbt::ser`). A data-carrying variant is rejected on write and so never
        // appears on the wire; selecting one here would still leave its fields
        // unset, which `Partial::build` then reports on its own.
        Type::User(UserType::Enum(_)) => read_enum::<F, R>(p, shape, tag, r),
        _ => Err(unsupported("deserialization of this type is not supported")),
    }
}

/// Reads a unit enum variant. See [`crate::reflect::VariantAs`] for the modes.
#[inline(never)]
fn read_enum<'f, F: EndiannessImpl, R: ReadBytesExt>(
    p: Part<'f>,
    shape: &'static Shape,
    tag: FieldType,
    r: &mut R,
) -> Result<Part<'f>, Error> {
    let mode = VariantAs::of(shape)?;
    let lenient = Lenient::of_enum(shape, mode)?;
    let expected = mode.tag();
    if tag != expected {
        // A tag the enum's own `#[facet(nbtx::lenient_width(...))]` names is
        // converted into the mode's width instead of being refused.
        if lenient.accepts(tag) {
            let disc = lenient_discriminant(read_wire::<F, R>(r, tag)?, mode)?;
            return p.select_variant(disc).map_err(reflect_err);
        }
        return Err(unexpected_type(expected, tag));
    }
    if mode == VariantAs::Str {
        let name = io::read_str_payload::<F, R>(r)?;
        let name = bstr::BStr::new(&name).to_string();
        // Rename-aware: `select_variant_named` matches a variant's effective
        // name, which is what the writer emitted.
        return p.select_variant_named(&name).map_err(reflect_err);
    }
    // The tag types are signed; `widen` reinterprets the bit pattern for the
    // unsigned modes so that the discriminant comes back as it was written.
    let raw = match expected {
        FieldType::Byte => i64::from(io::read_i8(r)?),
        FieldType::Short => i64::from(io::read_i16::<F, R>(r)?),
        FieldType::Int => i64::from(io::read_i32::<F, R>(r)?),
        _ => io::read_i64::<F, R>(r)?,
    };
    p.select_variant(mode.widen(raw)).map_err(reflect_err)
}

/// Reads one scalar payload at the width `tag` names, without regard to what it
/// is going to be stored in.
///
/// Only reached through a `#[facet(nbtx::lenient_width(...))]` declaration,
/// which can only name the six scalar tags — so the fallback arm is
/// unreachable, and is written as the tag mismatch it would be rather than a
/// panic.
#[inline(never)]
fn read_wire<F: EndiannessImpl, R: ReadBytesExt>(
    r: &mut R,
    tag: FieldType,
) -> Result<WireScalar, Error> {
    Ok(match tag {
        FieldType::Byte => WireScalar::Byte(io::read_i8(r)?),
        FieldType::Short => WireScalar::Short(io::read_i16::<F, R>(r)?),
        FieldType::Int => WireScalar::Int(io::read_i32::<F, R>(r)?),
        FieldType::Long => WireScalar::Long(io::read_i64::<F, R>(r)?),
        FieldType::Float => WireScalar::Float(io::read_f32::<F, R>(r)?),
        FieldType::Double => WireScalar::Double(io::read_f64::<F, R>(r)?),
        other => return Err(unexpected_type(FieldType::Byte, other)),
    })
}

#[inline(never)]
fn read_scalar<'f, F: EndiannessImpl, R: ReadBytesExt>(
    p: Part<'f>,
    scalar: ScalarType,
    tag: FieldType,
    r: &mut R,
    lenient: Lenient,
) -> Result<Part<'f>, Error> {
    // `#[facet(nbtx::lenient_width(...))]` names the other tags this field
    // takes; each is read at its own width and then converted losslessly into
    // the declared type, or reported. The field's own tag never gets here, so a
    // listed tag that happens to *be* the natural one changes nothing.
    if lenient.accepts(tag) && scalar_tag(scalar) != Some(tag) {
        return set_lenient(p, scalar, read_wire::<F, R>(r, tag)?);
    }

    macro_rules! expect {
        ($ty:ident) => {
            if tag != FieldType::$ty {
                return Err(unexpected_type(FieldType::$ty, tag));
            }
        };
    }

    match scalar {
        ScalarType::Bool => {
            expect!(Byte);
            // Only `0x01` is `true`; every other byte, including 0x02, is
            // `false`. TAG_Byte is a signed 8-bit integer that happens to be
            // used as a flag, and a `!= 0` test would disagree with the Bedrock
            // decoders that produced the data.
            p.set(io::read_i8(r)? == 1).map_err(reflect_err)
        }
        ScalarType::I8 => {
            expect!(Byte);
            p.set(io::read_i8(r)?).map_err(reflect_err)
        }
        ScalarType::U8 => {
            expect!(Byte);
            p.set(io::read_i8(r)?.cast_unsigned()).map_err(reflect_err)
        }
        ScalarType::I16 => {
            expect!(Short);
            p.set(io::read_i16::<F, R>(r)?).map_err(reflect_err)
        }
        ScalarType::I32 => {
            expect!(Int);
            p.set(io::read_i32::<F, R>(r)?).map_err(reflect_err)
        }
        ScalarType::I64 => {
            expect!(Long);
            p.set(io::read_i64::<F, R>(r)?).map_err(reflect_err)
        }
        ScalarType::F32 => {
            expect!(Float);
            p.set(io::read_f32::<F, R>(r)?).map_err(reflect_err)
        }
        ScalarType::F64 => {
            expect!(Double);
            p.set(io::read_f64::<F, R>(r)?).map_err(reflect_err)
        }
        ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
            expect!(String);
            let bytes = io::read_str_payload::<F, R>(r)?;
            // A concrete `String`/`&str` field validates plain UTF-8 and errors
            // otherwise; an NBT string that is not valid UTF-8 needs a
            // `bstr::BString` field or `Value`.
            p.set(String::from_utf8(bytes)?).map_err(reflect_err)
        }
        _ => Err(unsupported(
            "deserialization of this scalar type is not supported",
        )),
    }
}

/// Normalises a sequence tag into `(element tag, length)`; a `List` carries its
/// own element type, the typed arrays imply theirs. Split out of [`read_seq`] to
/// keep that recursive frame small.
#[inline(never)]
fn read_seq_header<F: EndiannessImpl, R: ReadBytesExt>(
    tag: FieldType,
    r: &mut R,
) -> Result<(FieldType, usize), Error> {
    Ok(match tag {
        FieldType::List => {
            let elem_tag = io::read_tag(r)?;
            let len = io::read_seq_len::<F, R>(r)? as usize;
            (elem_tag, len)
        }
        FieldType::ByteArray => (FieldType::Byte, io::read_seq_len::<F, R>(r)? as usize),
        FieldType::IntArray => (FieldType::Int, io::read_seq_len::<F, R>(r)? as usize),
        FieldType::LongArray => (FieldType::Long, io::read_seq_len::<F, R>(r)? as usize),
        other => return Err(unexpected_type(FieldType::List, other)),
    })
}

/// Parses and discards one value's payload, keeping the stream in sync.
///
/// Split out (and never inlined) so the 80-byte `Value` temporary it needs stays
/// out of the frames of the recursive `read_struct`/`read_map`.
#[inline(never)]
fn skip_value<F: EndiannessImpl, R: ReadBytesExt>(
    r: &mut R,
    tag: FieldType,
    depth: usize,
) -> Result<(), Error> {
    io::read_value::<F, R>(r, tag, depth)?;
    Ok(())
}

/// Reads a `List`/`ByteArray`/`IntArray`/`LongArray` tag into a `Vec`
/// (`array_len = None`) or fixed-size array (`array_len = Some(n)`).
fn read_seq<'f, F: EndiannessImpl, R: ReadBytesExt>(
    p: Part<'f>,
    elem_shape: &'static Shape,
    tag: FieldType,
    r: &mut R,
    array_len: Option<usize>,
    depth: usize,
    lenient: Lenient,
) -> Result<Part<'f>, Error> {
    io::check_depth(depth)?;
    let (elem_tag, len) = read_seq_header::<F, R>(tag, r)?;

    if let Some(n) = array_len {
        if len != n {
            return Err(unexpected_type(FieldType::List, tag));
        }
        let mut p = p.init_array().map_err(reflect_err)?;
        for i in 0..len {
            p = p.begin_nth_field(i).map_err(reflect_err)?;
            p = read_into::<F, R>(p, elem_shape, elem_tag, r, depth + 1, lenient)?;
            p = p.end().map_err(reflect_err)?;
        }
        Ok(p)
    } else {
        let mut p = p.init_list().map_err(reflect_err)?;
        for _ in 0..len {
            p = p.begin_list_item().map_err(reflect_err)?;
            p = read_into::<F, R>(p, elem_shape, elem_tag, r, depth + 1, lenient)?;
            p = p.end().map_err(reflect_err)?;
        }
        Ok(p)
    }
}

/// Reads a `Compound` into a map. Duplicate keys are first-wins, matching
/// [`io::read_compound`].
fn read_map<'f, F: EndiannessImpl, R: ReadBytesExt>(
    p: Part<'f>,
    _key_shape: &'static Shape,
    value_shape: &'static Shape,
    tag: FieldType,
    r: &mut R,
    depth: usize,
) -> Result<Part<'f>, Error> {
    if tag != FieldType::Compound {
        return Err(unexpected_type(FieldType::Compound, tag));
    }
    io::check_depth(depth)?;
    let mut seen: Vec<String> = Vec::new();
    let mut p = p.init_map().map_err(reflect_err)?;
    loop {
        let entry_tag = io::read_tag(r)?;
        if entry_tag == FieldType::End {
            break;
        }
        let key = String::from_utf8(io::read_str_payload::<F, R>(r)?)?;
        if seen.contains(&key) {
            // Duplicate key: keep the first, but still consume the payload so
            // the stream stays in sync.
            skip_value::<F, R>(r, entry_tag, depth + 1)?;
            continue;
        }
        seen.push(key.clone());
        p = p.begin_key().map_err(reflect_err)?;
        p = p.set(key).map_err(reflect_err)?;
        p = p.end().map_err(reflect_err)?;
        p = p.begin_value().map_err(reflect_err)?;
        // A map's values carry no `lenient_width` of their own: the attribute is
        // refused on a map-typed field in the first place.
        p = read_into::<F, R>(p, value_shape, entry_tag, r, depth + 1, Lenient::NONE)?;
        p = p.end().map_err(reflect_err)?;
    }
    Ok(p)
}

/// Reads a `Compound` into a `#[derive(Facet)]` struct.
///
/// * An unrecognised key is an [`Error::UnknownField`] unless the struct carries
///   `#[facet(nbtx::allow_unknown_fields)]`, in which case its payload is parsed
///   and discarded (the pre-4.0 behaviour).
/// * A key that repeats keeps the value of its *first* occurrence, matching
///   [`io::read_compound`].
fn read_struct<'f, F: EndiannessImpl, R: ReadBytesExt>(
    p: Part<'f>,
    shape: &'static Shape,
    tag: FieldType,
    r: &mut R,
    depth: usize,
) -> Result<Part<'f>, Error> {
    if tag != FieldType::Compound {
        return Err(unexpected_type(FieldType::Compound, tag));
    }
    io::check_depth(depth)?;
    let Type::User(UserType::Struct(st)) = shape.ty else {
        return Err(unsupported("expected a struct"));
    };
    let allow_unknown = crate::has_nbtx_attr(shape, "allow_unknown_fields");

    let mut filled = vec![false; st.fields.len()];
    let mut p = p;
    loop {
        let entry_tag = io::read_tag(r)?;
        if entry_tag == FieldType::End {
            break;
        }
        let key = io::read_str_payload::<F, R>(r)?;
        // Match against each field's effective (rename-aware) name.
        let field = st
            .fields
            .iter()
            .enumerate()
            .find(|(_, f)| f.effective_name().as_bytes() == key.as_slice());

        match field {
            Some((idx, _)) if filled[idx] => {
                // Duplicate key: first occurrence wins, discard this payload.
                skip_value::<F, R>(r, entry_tag, depth + 1)?;
            }
            Some((idx, f)) => {
                filled[idx] = true;
                let lenient = Lenient::of_field(shape, f)?;
                p = p.begin_nth_field(idx).map_err(reflect_err)?;
                p = read_into::<F, R>(p, f.shape(), entry_tag, r, depth + 1, lenient)?;
                p = p.end().map_err(reflect_err)?;
            }
            None if allow_unknown => {
                // Opted out of strict decoding: consume and discard the value.
                skip_value::<F, R>(r, entry_tag, depth + 1)?;
            }
            None => return Err(unknown_field(shape, &key)),
        }
    }
    Ok(p)
}

/// Reads a single value of type `T` from the reader.
///
/// # Root handling
///
/// Any root tag except `TAG_End` is accepted, and the root *name* is discarded —
/// exactly the mirror image of what [`to_bytes`](crate::to_bytes) writes, so
/// every `T` nbtx can encode it can also decode. To read the root name, decode
/// into a [`Named<T>`](crate::Named).
pub fn from_bytes<'f, F: EndiannessImpl, T: Facet<'f>>(
    reader: &mut impl ReadBytesExt,
) -> Result<T, Error> {
    let root_tag = io::read_tag(reader)?;
    if root_tag == FieldType::End {
        // A lone `TAG_End` is the compound terminator, not a document.
        return Err(Error::UnexpectedEnd(UnexpectedEnd {
            #[cfg(feature = "error-context")]
            at: String::from("root"),
            #[cfg(feature = "error-context")]
            index: None,
        }));
    }
    let root_name = io::read_str_payload::<F, _>(reader)?;

    let shape = <T as Facet>::SHAPE;
    let p = Partial::alloc::<T>().map_err(reflect_err)?;

    // `Named<T>` captures the root name rather than discarding it.
    let p = if let Some(nr) = named::as_named_root(shape) {
        let p = p.begin_nth_field(nr.name_idx).map_err(reflect_err)?;
        let p = p
            .set(bstr::BString::from(root_name))
            .map_err(reflect_err)?
            .end()
            .map_err(reflect_err)?;
        let p = p.begin_nth_field(nr.value_idx).map_err(reflect_err)?;
        read_into::<F, _>(p, nr.value_shape, root_tag, reader, 0, Lenient::NONE)?
            .end()
            .map_err(reflect_err)?
    } else {
        read_into::<F, _>(p, shape, root_tag, reader, 0, Lenient::NONE)?
    };

    let built = p.build().map_err(reflect_err)?;
    built.materialize::<T>().map_err(reflect_err)
}

macro_rules! endian_fn {
    ($from:ident, $endian:ty, $doc:literal) => {
        #[doc = $doc]
        pub fn $from<'f, T: Facet<'f>>(reader: &mut impl ReadBytesExt) -> Result<T, Error> {
            from_bytes::<$endian, T>(reader)
        }
    };
}

endian_fn!(
    from_be_bytes,
    BigEndian,
    "Reads a value from big-endian NBT (fixed-width big-endian integers)."
);
endian_fn!(
    from_le_bytes,
    LittleEndian,
    "Reads a value from little-endian NBT (Minecraft: Bedrock Edition, disk)."
);
endian_fn!(
    from_varint_bytes,
    VarintEndian,
    "Reads a value from varint little-endian NBT (Minecraft: Bedrock Edition, network)."
);
