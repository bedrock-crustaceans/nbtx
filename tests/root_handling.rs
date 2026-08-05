//! Root tag and root name handling.
//!
//! An NBT document is `<tag byte><root name><payload>`. nbtx's rules are:
//!
//! * the plain entry points always write an **empty** root name, for every `T`,
//!   so the wire format never depends on a Rust type or field identifier;
//! * `from_*_bytes` accepts **any** root tag except `TAG_End` and discards the
//!   name, so reading and writing are symmetric for every `T`;
//! * [`nbtx::Named<T>`] is the explicit opt-in to a real root name.

#![cfg(feature = "nbt")]

use bstr::BString;
use nbtx::{
    Compound, Named, Value, from_be_bytes, from_le_bytes, from_varint_bytes, to_be_bytes,
    to_le_bytes, to_varint_bytes,
};

fn hex(s: &str) -> Vec<u8> {
    let packed: String = s.split_whitespace().collect();
    assert!(packed.len().is_multiple_of(2), "odd-length hex: {s}");
    (0..packed.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&packed[i..i + 2], 16).unwrap())
        .collect()
}

fn comp(entries: &[(&str, Value)]) -> Value {
    let mut m = Compound::new();
    for (k, v) in entries {
        m.insert(BString::from(*k), v.clone());
    }
    Value::Compound(m)
}

/// Writes a big-endian `u16`-prefixed string payload.
fn str_payload(out: &mut Vec<u8>, s: &[u8]) {
    out.extend_from_slice(&(s.len() as u16).to_be_bytes());
    out.extend_from_slice(s);
}

/// A derived struct's root name is empty, not its Rust type identifier —
/// otherwise renaming a Rust type would change the bytes on disk.
#[test]
fn derived_struct_root_name_is_empty_little_endian() {
    #[derive(facet::Facet, Debug)]
    struct Foo {
        #[facet(rename = "A")]
        a: i32,
    }
    let got = to_le_bytes(&Foo { a: 1 }).unwrap();
    assert_eq!(
        got,
        hex("0a0000030100410100000000"),
        "a struct root must carry an empty root name"
    );
}

/// See [`derived_struct_root_name_is_empty_little_endian`]; this pins the same
/// rule for the big-endian variant, where the name prefix is byte-swapped.
#[test]
fn derived_struct_root_name_is_empty_big_endian() {
    #[derive(facet::Facet)]
    struct Foo {
        x: i32,
    }
    assert_eq!(
        to_be_bytes(&Foo { x: 5 }).unwrap(),
        hex("0a 00 00 03 00 01 78 00 00 00 05 00"),
    );
}

/// A `String` root is valid NBT (tag 8 at the top level), and must both encode
/// and decode. nbtx used to write such roots while rejecting them on read — an
/// asymmetry that made it unable to decode its own output.
#[test]
fn non_compound_root_roundtrips() {
    let be = hex("080000000568656c6c6f");
    let le = hex("080000050068656c6c6f");
    let var = hex("08000568656c6c6f");
    let expect = Value::String("hello".into());

    assert_eq!(to_be_bytes(&expect).unwrap(), be);
    assert_eq!(to_le_bytes(&expect).unwrap(), le);
    assert_eq!(to_varint_bytes(&expect).unwrap(), var);

    assert_eq!(from_be_bytes::<Value>(&mut be.as_slice()).unwrap(), expect);
    assert_eq!(from_le_bytes::<Value>(&mut le.as_slice()).unwrap(), expect);
    assert_eq!(
        from_varint_bytes::<Value>(&mut var.as_slice()).unwrap(),
        expect
    );

    // A plain Rust `String` root works the same way.
    assert_eq!(
        from_be_bytes::<String>(&mut be.as_slice()).unwrap(),
        "hello"
    );
    assert_eq!(to_be_bytes(&String::from("hello")).unwrap(), be);
}

/// The same for a scalar root.
#[test]
fn scalar_root_roundtrips() {
    let var = hex("03000e"); // tag 3, empty name, zigzag(7) = 14
    assert_eq!(to_varint_bytes(&Value::Int(7)).unwrap(), var);
    assert_eq!(
        from_varint_bytes::<Value>(&mut var.as_slice()).unwrap(),
        Value::Int(7)
    );
}

/// A TAG_Int root produced elsewhere must decode, not just one nbtx wrote
/// itself.
#[test]
fn scalar_root_decodes_from_foreign_bytes() {
    let v: Value = from_le_bytes(&mut hex("03 00 00 05 00 00 00").as_slice()).unwrap();
    assert_eq!(v, Value::Int(5));
}

/// A hand-built TAG_Int root with an empty name.
#[test]
fn non_compound_root_decodes() {
    let mut buf = vec![3u8];
    str_payload(&mut buf, b"");
    buf.extend_from_slice(&123i32.to_be_bytes());

    let back: Value = from_be_bytes(&mut buf.as_slice())
        .expect("a non-compound root is valid NBT and must decode");
    assert_eq!(back, Value::Int(123));
}

/// Regression for the write/read asymmetry: **every** `Value` variant that can
/// appear at the root must survive `to_bytes` -> `from_bytes` unchanged, in all
/// three variants. Before 4.0 only `Compound` could be read back.
#[test]
fn every_value_variant_roundtrips_as_a_root() {
    let cases = [
        Value::Byte(-3),
        Value::Short(-300),
        Value::Int(i32::MIN),
        Value::Long(i64::MAX),
        Value::Float(1.5),
        Value::Double(-2.25),
        Value::ByteArray(vec![1, 2, 3]),
        Value::String("hello".into()),
        Value::List(vec![Value::Int(1), Value::Int(2)]),
        comp(&[("a", Value::Int(1))]),
        Value::IntArray(vec![-1, 300]),
        Value::LongArray(vec![-1, 300]),
    ];
    for v in cases {
        let be = to_be_bytes(&v).unwrap();
        assert_eq!(
            from_be_bytes::<Value>(&mut be.as_slice()).unwrap(),
            v,
            "BE {v:?}"
        );
        let le = to_le_bytes(&v).unwrap();
        assert_eq!(
            from_le_bytes::<Value>(&mut le.as_slice()).unwrap(),
            v,
            "LE {v:?}"
        );
        let var = to_varint_bytes(&v).unwrap();
        assert_eq!(
            from_varint_bytes::<Value>(&mut var.as_slice()).unwrap(),
            v,
            "varint {v:?}"
        );
        // The root name is always empty, so byte 1 (BE/LE) is the length prefix.
        assert_eq!(be[1..3], [0, 0], "root name must be empty for {v:?}");
    }
}

/// The header is always present: root tag byte, then the (empty) root name.
/// nbtx has no headless mode.
#[test]
fn root_always_has_header() {
    let bytes = to_be_bytes(&comp(&[("int", Value::Int(123))])).unwrap();
    assert_eq!(bytes[0], 10, "root tag byte is TAG_Compound");
    assert_eq!(&bytes[1..3], &[0, 0], "empty root name (u16 length 0)");
}

/// Reading into a bare `Value` discards the root name by design — `Value` has
/// nowhere to keep it. Two documents differing only in root name decode alike.
#[test]
fn root_name_is_discarded_without_the_named_wrapper() {
    let mut named = vec![10u8];
    str_payload(&mut named, b"hello");
    named.push(3);
    str_payload(&mut named, b"x");
    named.extend_from_slice(&7i32.to_be_bytes());
    named.push(0);

    let back: Value = from_be_bytes(&mut named.as_slice()).unwrap();
    let re = to_be_bytes(&back).unwrap();
    assert_eq!(&re[1..3], &[0, 0], "the re-encode has an empty root name");
    assert_eq!(back, comp(&[("x", Value::Int(7))]));
}

/// `Named<T>` is the way to keep the name: it round-trips a named-root document
/// byte-for-byte.
#[test]
fn named_wrapper_preserves_the_root_name() {
    let mut buf = vec![10u8];
    str_payload(&mut buf, b"hello world");
    buf.push(3);
    str_payload(&mut buf, b"x");
    buf.extend_from_slice(&1i32.to_be_bytes());
    buf.push(0);

    let back: Named<Value> = from_be_bytes(&mut buf.as_slice()).unwrap();
    assert_eq!(back.name, "hello world");
    assert_eq!(
        to_be_bytes(&back).unwrap(),
        buf,
        "the root compound's name must survive a round-trip"
    );

    // Without the wrapper the name is dropped and re-emitted empty, by design.
    let bare: Value = from_be_bytes(&mut buf.as_slice()).unwrap();
    assert_eq!(&to_be_bytes(&bare).unwrap()[1..3], &[0, 0]);
}

/// nbtx can decode its own non-compound root output — the write and read paths
/// accept the same set of root tags.
#[test]
fn root_type_symmetry() {
    let bytes = to_be_bytes(&Value::Int(5)).unwrap();
    assert_eq!(bytes[0], 3, "wrote a non-compound (TAG_Int) root");
    let back: Value = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(back, Value::Int(5));
}

/// `Named<T>` must round-trip the name for any `T`, in all three variants.
#[test]
fn named_root_roundtrips_its_name() {
    let doc = Named {
        name: "hello world".into(),
        value: comp(&[("x", Value::Int(1))]),
    };

    let be = to_be_bytes(&doc).unwrap();
    // 0a | u16 len 11 | "hello world" | ...
    assert_eq!(be[0], 0x0a);
    assert_eq!(&be[1..3], &[0x00, 0x0b]);
    assert_eq!(&be[3..14], b"hello world");

    for (label, bytes, back) in [
        (
            "BE",
            be.clone(),
            from_be_bytes::<Named<Value>>(&mut be.as_slice()),
        ),
        (
            "LE",
            to_le_bytes(&doc).unwrap(),
            from_le_bytes::<Named<Value>>(&mut to_le_bytes(&doc).unwrap().as_slice()),
        ),
        (
            "varint",
            to_varint_bytes(&doc).unwrap(),
            from_varint_bytes::<Named<Value>>(&mut to_varint_bytes(&doc).unwrap().as_slice()),
        ),
    ] {
        let back = back.unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_eq!(back.name, doc.name, "{label}: root name lost");
        assert_eq!(back.value, doc.value, "{label}: payload changed");
        assert!(!bytes.is_empty());
    }

    // A non-compound payload works the same way.
    let scalar = Named {
        name: "n".into(),
        value: Value::Int(7),
    };
    let bytes = to_be_bytes(&scalar).unwrap();
    let back: Named<Value> = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(back, scalar);

    // ...and so does a statically-typed payload.
    #[derive(facet::Facet, Debug, PartialEq)]
    struct Foo {
        #[facet(rename = "A")]
        a: i32,
    }
    let typed = Named {
        name: "root".into(),
        value: Foo { a: 5 },
    };
    let bytes = to_le_bytes(&typed).unwrap();
    let back: Named<Foo> = from_le_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(back, typed);
}

/// A user's own struct that is *shaped exactly like* `nbtx::Named<T>` — same
/// name, same two fields, same field types — must encode as an ordinary
/// compound, not be hijacked into root-name wire semantics.
///
/// The wrapper is detected by `Shape::decl_id` (facet's type-parameter-erased
/// declaration identity), so a lookalike declared in another crate or module
/// never matches. A purely structural match on `type_identifier == "Named"`
/// plus the field shapes — which is what nbtx did before — silently captured
/// types like this one.
#[test]
fn a_lookalike_named_struct_is_not_treated_as_a_root_name() {
    // Declared in its own module purely so the name can be *literally* `Named`
    // without shadowing the import above.
    mod lookalike {
        #[derive(facet::Facet, Debug, PartialEq)]
        pub struct Named<T> {
            pub name: bstr::BString,
            pub value: T,
        }
    }

    let mine = lookalike::Named::<i32> {
        name: "hello world".into(),
        value: 7,
    };
    let bytes = to_be_bytes(&mine).unwrap();

    // Root name is empty (the plain-entry-point rule), and the payload is a
    // two-key compound — i.e. it went down the ordinary struct path.
    assert_eq!(bytes[0], 0x0a, "root tag must be TAG_Compound");
    assert_eq!(&bytes[1..3], &[0x00, 0x00], "root name must be empty");

    let as_value: Value = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(
        as_value,
        comp(&[
            ("name", Value::String("hello world".into())),
            ("value", Value::Int(7)),
        ]),
        "a lookalike must encode as a plain two-key compound"
    );

    // The real wrapper, with identical contents, writes the name into the root
    // header instead — the behaviour the lookalike must *not* inherit.
    let real = Named {
        name: "hello world".into(),
        value: 7i32,
    };
    let real_bytes = to_be_bytes(&real).unwrap();
    assert_eq!(real_bytes[0], 0x03, "root tag is the payload's own tag");
    assert_eq!(&real_bytes[3..14], b"hello world");
    assert_ne!(bytes, real_bytes);
}
