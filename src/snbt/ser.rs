//! Stringified NBT (SNBT) serialization, driven by `facet` reflection.
//!
//! Mirrors the binary serializer's tag conventions but emits the human-readable
//! SNBT text used in Minecraft commands:
//!
//! * scalars use type suffixes: `b` (byte), `s` (short), `l` (long), `f`
//!   (float), `d` (double); `i32` has no suffix; `bool` renders `true`/`false`
//! * strings render as `"quoted"` text; a `bstr::BString`/`BStr` field is a
//!   string too (rendered lossily, since SNBT is text), not a byte array
//! * `Vec<u8>`/`ByteArray` → `[B;1b,2b,..]`, `Vec<i32>`/`IntArray` →
//!   `[I;1,2,..]`, `Vec<i64>`/`LongArray` → `[L;1l,2l,..]`, other lists → `[..]`
//! * struct/map/`Compound` → `{k:v,..}`
//! * a unit enum variant → whatever its mandatory
//!   `#[facet(nbtx::variant_as(<mode>))]` declares: its (rename-aware) name as a
//!   quoted string, or its discriminant as an integer literal carrying the
//!   suffix of the tag that would hold it (`2b`, `2s`, `2`, `2l`)

use bstr::ByteSlice;
use facet::Facet;
use facet_core::{Def, ScalarType, Type, UserType};
use facet_reflect::Peek;

// A `BString`/`BStr` renders as a quoted string rather than a `[B;..]`
// byte-array literal, and a `Value` is detected by shape id: the same rules the
// binary codec applies, shared from `crate::reflect` so the two cannot drift.
use crate::reflect::{EnumWire, enum_wire, is_bstring, is_value, reflect_err, unsupported};
use crate::{Error, FieldType, Value, check_depth};

/// Serializes `value` into an SNBT string.
pub fn to_string<'f, T: Facet<'f> + ?Sized>(value: &'f T) -> Result<String, Error> {
    let mut ser = Serializer::new();
    ser.serialize(value)?;
    Ok(ser.into_inner())
}

/// An SNBT serializer.
#[derive(Default)]
pub struct Serializer {
    pub(crate) output: String,
}

impl Serializer {
    /// Creates a new, empty serializer.
    #[must_use]
    pub fn new() -> Serializer {
        Serializer {
            output: String::new(),
        }
    }

    /// Consumes the serializer and returns the output string.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.output
    }

    /// Serializes a value, appending to the internal buffer.
    pub fn serialize<'f, T: Facet<'f> + ?Sized>(&mut self, value: &'f T) -> Result<(), Error> {
        render(&mut self.output, Peek::new(value), 0)
    }
}

fn quote_string(out: &mut String, s: &str) {
    out.reserve(s.len() + 2);
    out.push('"');
    // Escape the two characters that would otherwise terminate or corrupt the
    // quoted literal. The reader performs the inverse un-escaping.
    for c in s.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
}

/// Renders a compound key: bare if it is a "simple" identifier, otherwise
/// quoted.
fn render_key(out: &mut String, key: &str) {
    let simple = !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'+' | b'-'));
    if simple {
        out.push_str(key);
    } else {
        quote_string(out, key);
    }
}

/// Renders a dynamic [`Value`] into `out`.
///
/// `depth` is the number of containers already entered; the two recursive arms
/// check it against [`MAX_DEPTH`](crate::MAX_DEPTH) before descending, so a
/// deeply nested `Value` built in memory yields [`Error::MaxDepthExceeded`]
/// rather than overflowing the stack, mirroring `nbt::io::write_value`. The
/// bound applies to the writer as well as the parser: a `Value` can be built in
/// memory to any depth, so encoding is just as exposed as decoding.
fn render_value(out: &mut String, v: &Value, depth: usize) -> Result<(), Error> {
    match v {
        Value::Byte(n) => {
            out.push_str(&n.to_string());
            out.push('b');
        }
        Value::Short(n) => {
            out.push_str(&n.to_string());
            out.push('s');
        }
        Value::Int(n) => out.push_str(&n.to_string()),
        Value::Long(n) => {
            out.push_str(&n.to_string());
            out.push('l');
        }
        Value::Float(n) => {
            out.push_str(&n.to_string());
            out.push('f');
        }
        Value::Double(n) => {
            out.push_str(&n.to_string());
            out.push('d');
        }
        Value::String(s) => quote_string(out, &s.to_str_lossy()),
        Value::ByteArray(bytes) => {
            out.push_str("[B;");
            for (i, b) in bytes.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&b.cast_signed().to_string());
                out.push('b');
            }
            out.push(']');
        }
        Value::IntArray(ints) => {
            out.push_str("[I;");
            for (i, n) in ints.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&n.to_string());
            }
            out.push(']');
        }
        Value::LongArray(longs) => {
            out.push_str("[L;");
            for (i, n) in longs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&n.to_string());
                out.push('l');
            }
            out.push(']');
        }
        Value::List(items) => {
            check_depth(depth)?;
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                render_value(out, item, depth + 1)?;
            }
            out.push(']');
        }
        Value::Compound(map) => {
            check_depth(depth)?;
            out.push('{');
            for (i, (k, v)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                render_key(out, &k.to_str_lossy());
                out.push(':');
                render_value(out, v, depth + 1)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

/// Renders any `facet`-reflected value into `out`. `depth` carries the same
/// meaning as in [`render_value`].
fn render(out: &mut String, peek: Peek, depth: usize) -> Result<(), Error> {
    let shape = peek.shape();

    if is_value(shape) {
        return render_value(out, peek.get::<Value>().map_err(reflect_err)?, depth);
    }

    // A `bstr::BString` field is a String tag, not a byte array. SNBT is text,
    // so its raw bytes are rendered lossily, exactly as `Value::String` is.
    if is_bstring(shape) {
        let s: &bstr::BString = peek.get::<bstr::BString>().map_err(reflect_err)?;
        quote_string(out, &s.to_str_lossy());
        return Ok(());
    }

    if let Def::Option(_) = shape.def {
        let opt = peek.into_option().map_err(reflect_err)?;
        return match opt.value() {
            Some(inner) => render(out, inner, depth),
            None => Err(unsupported("cannot serialize a `None` value here")),
        };
    }

    if let Some(scalar) = ScalarType::try_from_shape(shape) {
        return render_scalar(out, peek, scalar);
    }

    if matches!(shape.def, Def::List(_) | Def::Array(_) | Def::Slice(_)) {
        return render_seq(out, peek, depth);
    }

    if let Def::Map(_) = shape.def {
        return render_map(out, peek, depth);
    }

    match shape.ty {
        Type::User(UserType::Struct(_)) => render_struct(out, peek, depth),
        Type::User(UserType::Enum(_)) => render_enum(out, peek),
        _ => Err(unsupported("serialization of this type is not supported")),
    }
}

/// Renders a unit enum variant in whichever form its mandatory
/// `#[facet(nbtx::variant_as(...))]` declared.
///
/// `str` mode writes the variant's (rename-aware) name as a quoted string; an
/// integer mode writes its discriminant with the type suffix of the tag that
/// carries it — `b` for `u8`/`i8`, `s` for `u16`/`i16`, none for `u32`/`i32`,
/// `l` for `u64`/`i64` — so the literal is indistinguishable from a plain
/// `Byte`/`Short`/`Int`/`Long` scalar of the same value, and reads back as one.
fn render_enum(out: &mut String, peek: Peek) -> Result<(), Error> {
    match enum_wire(peek)? {
        EnumWire::Name(name) => quote_string(out, name),
        EnumWire::Int(tag, v) => {
            out.push_str(&v.to_string());
            match tag {
                FieldType::Byte => out.push('b'),
                FieldType::Short => out.push('s'),
                FieldType::Int => {}
                // `enum_wire` only ever answers with these four tags.
                _ => out.push('l'),
            }
        }
    }
    Ok(())
}

fn render_scalar(out: &mut String, peek: Peek, scalar: ScalarType) -> Result<(), Error> {
    match scalar {
        ScalarType::Bool => out.push_str(if *peek.get::<bool>().map_err(reflect_err)? {
            "true"
        } else {
            "false"
        }),
        ScalarType::I8 => {
            out.push_str(&peek.get::<i8>().map_err(reflect_err)?.to_string());
            out.push('b');
        }
        ScalarType::U8 => {
            out.push_str(
                &peek
                    .get::<u8>()
                    .map_err(reflect_err)?
                    .cast_signed()
                    .to_string(),
            );
            out.push('b');
        }
        ScalarType::I16 => {
            out.push_str(&peek.get::<i16>().map_err(reflect_err)?.to_string());
            out.push('s');
        }
        ScalarType::I32 => out.push_str(&peek.get::<i32>().map_err(reflect_err)?.to_string()),
        ScalarType::I64 => {
            out.push_str(&peek.get::<i64>().map_err(reflect_err)?.to_string());
            out.push('l');
        }
        ScalarType::F32 => {
            out.push_str(&peek.get::<f32>().map_err(reflect_err)?.to_string());
            out.push('f');
        }
        ScalarType::F64 => {
            out.push_str(&peek.get::<f64>().map_err(reflect_err)?.to_string());
            out.push('d');
        }
        ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
            let s = peek
                .as_str()
                .ok_or_else(|| unsupported("expected a string value"))?;
            quote_string(out, s);
        }
        _ => {
            return Err(unsupported(
                "serialization of this scalar type is not supported",
            ));
        }
    }
    Ok(())
}

fn render_seq(out: &mut String, peek: Peek, depth: usize) -> Result<(), Error> {
    check_depth(depth)?;
    let shape = peek.shape();
    let elem_shape = match shape.def {
        Def::List(def) => def.t(),
        Def::Array(def) => def.t(),
        Def::Slice(def) => def.t(),
        _ => return Err(unsupported("expected a list, array or slice")),
    };
    let list = peek.into_list_like().map_err(reflect_err)?;
    let id = elem_shape.id;

    if id == <u8 as Facet>::SHAPE.id {
        out.push_str("[B;");
        for (i, item) in list.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(
                &item
                    .get::<u8>()
                    .map_err(reflect_err)?
                    .cast_signed()
                    .to_string(),
            );
            out.push('b');
        }
        out.push(']');
    } else if id == <i32 as Facet>::SHAPE.id {
        out.push_str("[I;");
        for (i, item) in list.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&item.get::<i32>().map_err(reflect_err)?.to_string());
        }
        out.push(']');
    } else if id == <i64 as Facet>::SHAPE.id {
        out.push_str("[L;");
        for (i, item) in list.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&item.get::<i64>().map_err(reflect_err)?.to_string());
            out.push('l');
        }
        out.push(']');
    } else {
        out.push('[');
        for (i, item) in list.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            render(out, item, depth + 1)?;
        }
        out.push(']');
    }
    Ok(())
}

fn render_struct(out: &mut String, peek: Peek, depth: usize) -> Result<(), Error> {
    check_depth(depth)?;
    let st = peek.into_struct().map_err(reflect_err)?;
    out.push('{');
    let mut first = true;
    for (i, field) in st.ty().fields.iter().enumerate() {
        let raw = st.field(i).map_err(reflect_err)?;
        // Skip `None` optional fields.
        let value = if let Def::Option(_) = raw.shape().def {
            match raw.into_option().map_err(reflect_err)?.value() {
                Some(inner) => inner,
                None => continue,
            }
        } else {
            raw
        };
        if !first {
            out.push(',');
        }
        first = false;
        render_key(out, field.effective_name());
        out.push(':');
        render(out, value, depth + 1)?;
    }
    out.push('}');
    Ok(())
}

fn render_map(out: &mut String, peek: Peek, depth: usize) -> Result<(), Error> {
    check_depth(depth)?;
    let map = peek.into_map().map_err(reflect_err)?;
    out.push('{');
    let mut first = true;
    for (key, value) in map.iter() {
        // Skip `None` optional values.
        let value = if let Def::Option(_) = value.shape().def {
            match value.into_option().map_err(reflect_err)?.value() {
                Some(inner) => inner,
                None => continue,
            }
        } else {
            value
        };
        if !first {
            out.push(',');
        }
        first = false;
        let key_str = key
            .as_str()
            .ok_or_else(|| unsupported("map keys must be strings"))?;
        render_key(out, key_str);
        out.push(':');
        render(out, value, depth + 1)?;
    }
    out.push('}');
    Ok(())
}
