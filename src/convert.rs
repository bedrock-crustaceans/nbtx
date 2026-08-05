//! Direct conversion between a typed `#[derive(Facet)]` value and the dynamic
//! [`Value`] tree, with no binary NBT in between.
//!
//! [`to_value`] walks a [`facet_reflect::Peek`] and builds a [`Value`];
//! [`from_value`] walks a [`Value`] and builds the target type through a
//! [`facet_reflect::Partial`]. Both apply exactly the type↔tag convention the
//! binary codec uses — the rules live in [`crate::reflect`] and are shared, not
//! re-stated here — so the tree these produce is indistinguishable from one
//! obtained by encoding to bytes and decoding them back into a `Value`.
//!
//! Neither function touches the wire format, so there is no endianness, no
//! length prefix and no varint anywhere in this module, and it is available
//! whether or not the `nbt`/`snbt` features are enabled.

use bstr::BString;
use facet::Facet;
use facet_core::{Def, ScalarType, Shape, Type, UserType};
use facet_reflect::{Partial, Peek};

use crate::reflect::{
    is_bstring, is_value, list_tag, reflect_err, scalar_tag, unexpected_type, unknown_field,
    unsupported, unwrap_option, value_tag,
};
use crate::{Compound, Error, FieldType, Value, check_depth};

type Part<'f> = Partial<'f, true>;

// --- to_value -------------------------------------------------------------

/// Converts any [`Facet`] value into a dynamic [`Value`] tree, without going
/// through the binary format.
///
/// The result is *structurally identical* to what encoding `value` and decoding
/// the bytes back into a `Value` would produce — the same tag for every node —
/// but nothing is ever serialised, so this is both cheaper and available with
/// `--no-default-features`.
///
/// # Tags
///
/// The NBT tag of each node comes from the Rust type, exactly as in
/// `to_bytes` (the tag table is stated once, in `crate::reflect`):
///
/// * `bool`/`i8`/`u8` → `Byte`, `i16` → `Short`, `i32` → `Int`, `i64` → `Long`,
///   `f32` → `Float`, `f64` → `Double`
/// * `String`/`&str`/[`bstr::BString`] → `String`
/// * `Vec<u8>`/`[u8; N]` → `ByteArray`, `Vec<i32>` → `IntArray`, `Vec<i64>` →
///   `LongArray`, any other list/array → `List`
/// * struct/map → `Compound`, and a unit enum variant → `String` of its name
/// * a [`Value`] passes through unchanged, keeping the exact tag of every child
/// * a `None` [`Option`] field is omitted from its compound
///
/// # Errors
///
/// * [`Error::Unsupported`] for a type with no NBT representation (an enum
///   variant carrying data, a map with non-string keys, a bare `None`).
/// * [`Error::MaxDepthExceeded`] if the value nests containers more deeply than
///   [`MAX_DEPTH`](crate::MAX_DEPTH), the same bound the binary codec enforces.
///
/// # Notes
///
/// [`Named<T>`](crate::Named) has no special meaning here: a `Value` tree has no
/// document root to name, so a `Named` converts as the ordinary two-field
/// compound it is. Nor is a heterogeneous [`Value::List`] rejected the way
/// `to_bytes` rejects it — there is no single element-type byte to write, so a
/// passed-through `Value` is simply left as it is.
///
/// # Examples
///
/// ```
/// use bstr::BStr;
/// use facet::Facet;
///
/// #[derive(Facet)]
/// struct Player {
///     name: String,
///     health: f32,
/// }
///
/// let value = nbtx::to_value(&Player { name: "Steve".into(), health: 20.0 })?;
/// let compound = value.as_compound().expect("a struct is a compound");
/// // Compound keys are `BString`, so look them up with a `&BStr`.
/// assert!(compound[BStr::new("name")] == "Steve");
/// assert!(compound[BStr::new("health")] == 20.0_f32);
/// # Ok::<(), nbtx::Error>(())
/// ```
pub fn to_value<'f>(value: &'f (impl Facet<'f> + ?Sized)) -> Result<Value, Error> {
    peek_to_value(Peek::new(value), 0)
}

/// Converts the value behind `peek` into a [`Value`].
///
/// `depth` is the number of containers already entered; it is checked against
/// [`MAX_DEPTH`](crate::MAX_DEPTH) before recursing, so a deeply nested value
/// errors out instead of overflowing the stack. The dispatch order mirrors
/// `nbt::ser::write_payload` exactly.
fn peek_to_value(peek: Peek, depth: usize) -> Result<Value, Error> {
    let shape = peek.shape();

    // Dynamic value: pass it through verbatim, preserving every child's tag.
    if is_value(shape) {
        let v: &Value = peek.get::<Value>().map_err(reflect_err)?;
        check_value_depth(v, depth)?;
        return Ok(v.clone());
    }

    // `bstr::BString` field: an NBT *string* of raw bytes, not a byte array.
    if is_bstring(shape) {
        let s: &BString = peek.get::<BString>().map_err(reflect_err)?;
        return Ok(Value::String(s.clone()));
    }

    // Options: unwrap `Some` (a lone `None` should have been skipped upstream).
    if let Def::Option(_) = shape.def {
        return match unwrap_option(peek)? {
            Some(inner) => peek_to_value(inner, depth),
            None => Err(unsupported("cannot serialize a `None` value here")),
        };
    }

    if let Some(scalar) = ScalarType::try_from_shape(shape) {
        return scalar_to_value(peek, scalar);
    }

    if matches!(shape.def, Def::List(_) | Def::Array(_) | Def::Slice(_)) {
        return seq_to_value(peek, depth);
    }

    // Maps and structs → compounds.
    if let Def::Map(_) = shape.def {
        return map_to_value(peek, depth);
    }
    match shape.ty {
        Type::User(UserType::Struct(_)) => struct_to_value(peek, depth),
        Type::User(UserType::Enum(_)) => enum_to_value(peek),
        _ => Err(unsupported("serialization of this type is not supported")),
    }
}

/// Enforces [`MAX_DEPTH`](crate::MAX_DEPTH) over an already-built [`Value`].
///
/// A `Value` field is copied wholesale rather than walked node by node, so its
/// own nesting would otherwise escape the bound that every other path is held
/// to — and `to_bytes` *does* reject such a value (`nbt::io::write_value` checks
/// as it writes). Checking here keeps the two paths in agreement about which
/// values are convertible at all.
fn check_value_depth(value: &Value, depth: usize) -> Result<(), Error> {
    match value {
        Value::List(items) => {
            check_depth(depth)?;
            for item in items {
                check_value_depth(item, depth + 1)?;
            }
        }
        Value::Compound(map) => {
            check_depth(depth)?;
            for item in map.values() {
                check_value_depth(item, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn scalar_to_value(peek: Peek, scalar: ScalarType) -> Result<Value, Error> {
    Ok(match scalar {
        // `true` is byte 1 and `false` byte 0, which is what the binary writer
        // emits and what `value_into_scalar` reads back.
        ScalarType::Bool => Value::Byte(i8::from(*peek.get::<bool>().map_err(reflect_err)?)),
        ScalarType::I8 => Value::Byte(*peek.get::<i8>().map_err(reflect_err)?),
        // The `Byte` tag is a signed 8-bit integer; a `u8` keeps its bit pattern,
        // exactly as the binary writer stores it.
        ScalarType::U8 => Value::Byte((*peek.get::<u8>().map_err(reflect_err)?).cast_signed()),
        ScalarType::I16 => Value::Short(*peek.get::<i16>().map_err(reflect_err)?),
        ScalarType::I32 => Value::Int(*peek.get::<i32>().map_err(reflect_err)?),
        ScalarType::I64 => Value::Long(*peek.get::<i64>().map_err(reflect_err)?),
        ScalarType::F32 => Value::Float(*peek.get::<f32>().map_err(reflect_err)?),
        ScalarType::F64 => Value::Double(*peek.get::<f64>().map_err(reflect_err)?),
        ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
            let s = peek
                .as_str()
                .ok_or_else(|| unsupported("expected a string value"))?;
            Value::String(BString::from(s))
        }
        _ => {
            return Err(unsupported(
                "serialization of this scalar type is not supported",
            ));
        }
    })
}

fn seq_to_value(peek: Peek, depth: usize) -> Result<Value, Error> {
    check_depth(depth)?;
    let shape = peek.shape();
    let elem_shape = match shape.def {
        Def::List(def) => def.t(),
        Def::Array(def) => def.t(),
        Def::Slice(def) => def.t(),
        _ => return Err(unsupported("expected a list, array or slice")),
    };
    let list = peek.into_list_like().map_err(reflect_err)?;
    let len = list.len();

    // The element type decides the tag of the whole sequence, exactly as in
    // `nbt::ser::write_seq`: the three typed arrays are not `List`s of scalars.
    Ok(match list_tag(elem_shape) {
        FieldType::ByteArray => {
            let mut out = Vec::with_capacity(len);
            for item in list.iter() {
                out.push(*item.get::<u8>().map_err(reflect_err)?);
            }
            Value::ByteArray(out)
        }
        FieldType::IntArray => {
            let mut out = Vec::with_capacity(len);
            for item in list.iter() {
                out.push(*item.get::<i32>().map_err(reflect_err)?);
            }
            Value::IntArray(out)
        }
        FieldType::LongArray => {
            let mut out = Vec::with_capacity(len);
            for item in list.iter() {
                out.push(*item.get::<i64>().map_err(reflect_err)?);
            }
            Value::LongArray(out)
        }
        _ => {
            let mut out = Vec::with_capacity(len);
            for item in list.iter() {
                out.push(peek_to_value(item, depth + 1)?);
            }
            Value::List(out)
        }
    })
}

fn struct_to_value(peek: Peek, depth: usize) -> Result<Value, Error> {
    check_depth(depth)?;
    let st = peek.into_struct().map_err(reflect_err)?;
    let mut out = Compound::new();
    for (i, field) in st.ty().fields.iter().enumerate() {
        let raw = st.field(i).map_err(reflect_err)?;
        // Skip `None` optional fields entirely, as the binary writer does.
        let Some(value) = unwrap_option(raw)? else {
            continue;
        };
        let value = peek_to_value(value, depth + 1)?;
        out.insert(BString::from(field.effective_name().as_bytes()), value);
    }
    Ok(Value::Compound(out))
}

fn map_to_value(peek: Peek, depth: usize) -> Result<Value, Error> {
    check_depth(depth)?;
    let map = peek.into_map().map_err(reflect_err)?;
    let mut out = Compound::new();
    for (key, value) in map.iter() {
        let Some(value) = unwrap_option(value)? else {
            continue;
        };
        let key = key
            .as_str()
            .ok_or_else(|| unsupported("map keys must be strings"))?;
        let value = peek_to_value(value, depth + 1)?;
        // First occurrence wins, matching `io::read_compound`. (A Rust map
        // cannot actually repeat a key; this only keeps the rule stated once.)
        out.entry(BString::from(key)).or_insert(value);
    }
    Ok(Value::Compound(out))
}

fn enum_to_value(peek: Peek) -> Result<Value, Error> {
    let en = peek.into_enum().map_err(reflect_err)?;
    let variant = en.active_variant().map_err(reflect_err)?;
    if variant.data.fields.is_empty() {
        // Unit variant → a `String` holding the variant name.
        Ok(Value::String(BString::from(variant.name)))
    } else {
        Err(unsupported(
            "serializing enums with data (other than `Value`) is not supported",
        ))
    }
}

// --- from_value -----------------------------------------------------------

/// Builds a typed [`Facet`] value from a dynamic [`Value`] tree, without going
/// through the binary format.
///
/// The inverse of [`to_value`], and the same reader `from_bytes` is: the *tag*
/// of each node says what data is there, the *target type* says how to store it,
/// and a mismatch between the two is an error rather than a silent coercion.
///
/// # Behaviour
///
/// * A [`Value`] target captures the subtree unchanged, tags and all.
/// * A struct matches compound keys against field names
///   (`#[facet(rename = "…")]`-aware). An unknown key is an error unless the
///   struct opts out with `#[facet(nbtx::allow_unknown_fields)]`; a missing
///   [`Option`] field defaults to `None`.
/// * `Vec<T>`/`[T; N]` accept `List`, `ByteArray`, `IntArray` and `LongArray`;
///   a map accepts a `Compound`.
/// * A `bool` is `true` only for byte `1` — every other byte, `2` included, is
///   `false`, because `TAG_Byte` is a signed integer used as a flag and a `!= 0`
///   test would disagree with the Bedrock decoders that wrote the data.
/// * A `String`/`&str` field requires valid UTF-8; use [`bstr::BString`] or
///   [`Value`] for NBT strings that may not be.
///
/// # Errors
///
/// * [`Error::UnexpectedType`] when a node's tag cannot fill the target field
///   (a `List` where an `i32` was expected, and so on).
/// * [`Error::UnknownField`] for a compound key with no matching field.
/// * [`Error::MaxDepthExceeded`] past [`MAX_DEPTH`](crate::MAX_DEPTH).
/// * [`Error::Unsupported`] for a target type with no NBT representation, and
///   [`Error::Other`] for a non-UTF-8 `String` field or a struct left with a
///   required field unset.
///
/// # Examples
///
/// ```
/// use facet::Facet;
/// use nbtx::{Compound, Value};
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Player {
///     name: String,
///     health: f32,
/// }
///
/// let value = Value::Compound(Compound::from_iter([
///     ("name".into(), Value::String("Steve".into())),
///     ("health".into(), Value::Float(20.0)),
/// ]));
/// let player: Player = nbtx::from_value(value)?;
/// assert_eq!(player, Player { name: "Steve".into(), health: 20.0 });
/// # Ok::<(), nbtx::Error>(())
/// ```
pub fn from_value<'f, T: Facet<'f>>(value: Value) -> Result<T, Error> {
    let p = Partial::alloc::<T>().map_err(reflect_err)?;
    let p = value_into(p, <T as Facet>::SHAPE, value, 0)?;
    let built = p.build().map_err(reflect_err)?;
    built.materialize::<T>().map_err(reflect_err)
}

/// Fills the current partial frame — whose target type is described by `shape` —
/// from `value`.
///
/// `depth` is the number of containers already entered; every nested-container
/// arm checks it against [`MAX_DEPTH`](crate::MAX_DEPTH) before recursing. The
/// dispatch order mirrors `nbt::de::read_into` exactly.
fn value_into<'f>(
    p: Part<'f>,
    shape: &'static Shape,
    value: Value,
    depth: usize,
) -> Result<Part<'f>, Error> {
    // A dynamic `Value` target swallows the whole (possibly nested) subtree.
    if is_value(shape) {
        check_value_depth(&value, depth)?;
        return p.set(value).map_err(reflect_err);
    }

    // Option: the key was present, so this is `Some`.
    if let Def::Option(def) = shape.def {
        let p = p.begin_some().map_err(reflect_err)?;
        let p = value_into(p, def.t(), value, depth)?;
        return p.end().map_err(reflect_err);
    }

    // `BString` reflects as a `Def::List<u8>` and scalars can carry a `Def` of
    // their own, so both are ruled out before the container dispatch below.
    if !is_bstring(shape) && ScalarType::try_from_shape(shape).is_none() {
        match shape.def {
            Def::List(def) => return value_into_seq(p, def.t(), value, None, depth),
            Def::Array(def) => return value_into_seq(p, def.t(), value, Some(def.n), depth),
            Def::Map(def) => return value_into_map(p, def.v(), value, depth),
            _ => {}
        }
        if let Type::User(UserType::Struct(_)) = shape.ty {
            return value_into_struct(p, shape, value, depth);
        }
    }

    value_into_leaf(p, shape, value)
}

/// All the non-recursive target types.
fn value_into_leaf<'f>(
    p: Part<'f>,
    shape: &'static Shape,
    value: Value,
) -> Result<Part<'f>, Error> {
    // `bstr::BString` field: take the string's raw bytes without UTF-8
    // validation (unlike a concrete `String`, which must be valid UTF-8).
    if is_bstring(shape) {
        return match value {
            Value::String(s) => p.set(s).map_err(reflect_err),
            other => Err(unexpected_type(FieldType::String, value_tag(&other))),
        };
    }

    if let Some(scalar) = ScalarType::try_from_shape(shape) {
        return value_into_scalar(p, scalar, value);
    }

    match shape.ty {
        // A unit enum variant is represented as a `String` holding the variant's
        // name (see `enum_to_value`); a data-carrying variant is rejected there
        // and so never appears in a `Value`. Selecting a data-carrying variant
        // by name here would still leave its fields unset, which
        // `Partial::build` then reports on its own.
        Type::User(UserType::Enum(_)) => match value {
            Value::String(name) => {
                let name = bstr::BStr::new(name.as_slice()).to_string();
                p.select_variant_named(&name).map_err(reflect_err)
            }
            other => Err(unexpected_type(FieldType::String, value_tag(&other))),
        },
        _ => Err(unsupported("deserialization of this type is not supported")),
    }
}

fn value_into_scalar(p: Part<'_>, scalar: ScalarType, value: Value) -> Result<Part<'_>, Error> {
    // The tag a value of this scalar type would have been *written* with is the
    // only tag it may be read from, so the check is the shared `scalar_tag`
    // table rather than a second, hand-maintained one.
    let expected = scalar_tag(scalar)
        .ok_or_else(|| unsupported("deserialization of this scalar type is not supported"))?;
    let actual = value_tag(&value);
    if actual != expected {
        return Err(unexpected_type(expected, actual));
    }

    match (scalar, value) {
        // Only `0x01` is `true`; every other byte, including 0x02, is `false`.
        // See `nbt::de::read_scalar` for why a `!= 0` test would be wrong.
        (ScalarType::Bool, Value::Byte(v)) => p.set(v == 1).map_err(reflect_err),
        (ScalarType::I8, Value::Byte(v)) => p.set(v).map_err(reflect_err),
        (ScalarType::U8, Value::Byte(v)) => p.set(v.cast_unsigned()).map_err(reflect_err),
        (ScalarType::I16, Value::Short(v)) => p.set(v).map_err(reflect_err),
        (ScalarType::I32, Value::Int(v)) => p.set(v).map_err(reflect_err),
        (ScalarType::I64, Value::Long(v)) => p.set(v).map_err(reflect_err),
        (ScalarType::F32, Value::Float(v)) => p.set(v).map_err(reflect_err),
        (ScalarType::F64, Value::Double(v)) => p.set(v).map_err(reflect_err),
        (ScalarType::Str | ScalarType::String | ScalarType::CowStr, Value::String(s)) => {
            // A concrete `String`/`&str` field validates plain UTF-8 and errors
            // otherwise; an NBT string that is not valid UTF-8 needs a
            // `bstr::BString` field or `Value`.
            p.set(String::from_utf8(Vec::from(s))?).map_err(reflect_err)
        }
        // Unreachable: the tag check above already pinned the pairing.
        _ => Err(unsupported(
            "deserialization of this scalar type is not supported",
        )),
    }
}

/// Reads a `List`/`ByteArray`/`IntArray`/`LongArray` into a `Vec`
/// (`array_len = None`) or fixed-size array (`array_len = Some(n)`).
///
/// The three typed arrays are accepted for *any* element type, exactly as
/// `nbt::de::read_seq` accepts them: the container tag only fixes the tag of the
/// elements, and the target type still decides how each is stored. Their
/// elements are re-tagged lazily so a large `ByteArray` never materialises as a
/// vector of `Value`s.
fn value_into_seq<'f>(
    p: Part<'f>,
    elem_shape: &'static Shape,
    value: Value,
    array_len: Option<usize>,
    depth: usize,
) -> Result<Part<'f>, Error> {
    check_depth(depth)?;
    let tag = value_tag(&value);
    match value {
        Value::List(items) => {
            let len = items.len();
            fill_seq(p, elem_shape, tag, len, items, array_len, depth)
        }
        Value::ByteArray(bytes) => {
            let len = bytes.len();
            let items = bytes.into_iter().map(|b| Value::Byte(b.cast_signed()));
            fill_seq(p, elem_shape, tag, len, items, array_len, depth)
        }
        Value::IntArray(ints) => {
            let len = ints.len();
            fill_seq(
                p,
                elem_shape,
                tag,
                len,
                ints.into_iter().map(Value::Int),
                array_len,
                depth,
            )
        }
        Value::LongArray(longs) => {
            let len = longs.len();
            fill_seq(
                p,
                elem_shape,
                tag,
                len,
                longs.into_iter().map(Value::Long),
                array_len,
                depth,
            )
        }
        _ => Err(unexpected_type(FieldType::List, tag)),
    }
}

/// Drains `items` (already re-tagged by [`value_into_seq`]) into a list or a
/// fixed-size array. `tag` is only carried through for the length-mismatch
/// error, which names the container tag that supplied the elements.
fn fill_seq<'f>(
    p: Part<'f>,
    elem_shape: &'static Shape,
    tag: FieldType,
    len: usize,
    items: impl IntoIterator<Item = Value>,
    array_len: Option<usize>,
    depth: usize,
) -> Result<Part<'f>, Error> {
    if let Some(n) = array_len {
        if len != n {
            return Err(unexpected_type(FieldType::List, tag));
        }
        let mut p = p.init_array().map_err(reflect_err)?;
        for (i, item) in items.into_iter().enumerate() {
            p = p.begin_nth_field(i).map_err(reflect_err)?;
            p = value_into(p, elem_shape, item, depth + 1)?;
            p = p.end().map_err(reflect_err)?;
        }
        Ok(p)
    } else {
        let mut p = p.init_list().map_err(reflect_err)?;
        for item in items {
            p = p.begin_list_item().map_err(reflect_err)?;
            p = value_into(p, elem_shape, item, depth + 1)?;
            p = p.end().map_err(reflect_err)?;
        }
        Ok(p)
    }
}

/// Reads a `Compound` into a map.
fn value_into_map<'f>(
    p: Part<'f>,
    value_shape: &'static Shape,
    value: Value,
    depth: usize,
) -> Result<Part<'f>, Error> {
    let Value::Compound(entries) = value else {
        return Err(unexpected_type(FieldType::Compound, value_tag(&value)));
    };
    check_depth(depth)?;
    let mut p = p.init_map().map_err(reflect_err)?;
    // A `Compound` cannot hold a duplicate key, so the byte reader's first-wins
    // rule has already been applied by whatever built this tree.
    for (key, entry) in entries {
        let key = String::from_utf8(Vec::from(key))?;
        p = p.begin_key().map_err(reflect_err)?;
        p = p.set(key).map_err(reflect_err)?;
        p = p.end().map_err(reflect_err)?;
        p = p.begin_value().map_err(reflect_err)?;
        p = value_into(p, value_shape, entry, depth + 1)?;
        p = p.end().map_err(reflect_err)?;
    }
    Ok(p)
}

/// Reads a `Compound` into a `#[derive(Facet)]` struct.
///
/// An unrecognised key is an [`Error::UnknownField`] unless the struct carries
/// `#[facet(nbtx::allow_unknown_fields)]`, in which case it is skipped. Fields
/// with no matching key are left unset; `Partial::build` then defaults `Option`
/// fields to `None` and reports any other omission.
fn value_into_struct<'f>(
    p: Part<'f>,
    shape: &'static Shape,
    value: Value,
    depth: usize,
) -> Result<Part<'f>, Error> {
    let Value::Compound(entries) = value else {
        return Err(unexpected_type(FieldType::Compound, value_tag(&value)));
    };
    check_depth(depth)?;
    let Type::User(UserType::Struct(st)) = shape.ty else {
        return Err(unsupported("expected a struct"));
    };
    let allow_unknown = crate::has_nbtx_attr(shape, "allow_unknown_fields");

    let mut p = p;
    for (key, entry) in entries {
        // Match against each field's effective (rename-aware) name.
        let field = st
            .fields
            .iter()
            .enumerate()
            .find(|(_, f)| f.effective_name().as_bytes() == key.as_slice());

        match field {
            Some((idx, f)) => {
                p = p.begin_nth_field(idx).map_err(reflect_err)?;
                p = value_into(p, f.shape(), entry, depth + 1)?;
                p = p.end().map_err(reflect_err)?;
            }
            // Opted out of strict decoding: drop the entry.
            None if allow_unknown => {}
            None => return Err(unknown_field(shape, key.as_slice())),
        }
    }
    Ok(p)
}
