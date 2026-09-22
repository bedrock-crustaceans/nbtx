//! Integration tests for the dynamic [`nbtx::Value`] codec.

#![cfg(feature = "nbt")]

use bstr::BString;
use nbtx::Compound;
use nbtx::{
    Value, ValueList, from_be_bytes, from_le_bytes, from_varint_bytes, to_be_bytes, to_le_bytes,
    to_varint_bytes,
};

macro_rules! for_each_endian {
    ($check:ident) => {
        $check!(to_be_bytes, from_be_bytes);
        $check!(to_le_bytes, from_le_bytes);
        $check!(to_varint_bytes, from_varint_bytes);
    };
}

/// Round-trips a compound holding *every* tag, asserting byte-identical
/// re-encoding and preserved tag identity for each endianness.
#[test]
fn value_every_tag_byte_identical() {
    let value = Value::Compound(Compound::from([
        ("byte".into(), Value::Byte(-5)),
        ("short".into(), Value::Short(1234)),
        ("int".into(), Value::Int(70_000)),
        ("long".into(), Value::Long(5_000_000_000)),
        ("float".into(), Value::Float(1.5)),
        ("double".into(), Value::Double(2.5)),
        (
            "byte_array".into(),
            Value::ByteArray(vec![0, 0x7f, 0x80, 0xff]),
        ),
        ("int_array".into(), Value::IntArray(vec![1, -2, 3])),
        ("long_array".into(), Value::LongArray(vec![100, -200])),
        ("string".into(), Value::String("Hello, World!".into())),
        ("list".into(), Value::List(ValueList::Int(vec![1, 2, 3]))),
        (
            "compound".into(),
            Value::Compound(Compound::from([("k".into(), Value::Byte(1))])),
        ),
    ]));

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&value).unwrap();
            let back: Value = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, value);
            let re = $to(&back).unwrap();
            assert_eq!(re, bytes, "byte-identical re-encode");

            let c = back.as_compound().unwrap();
            assert!(c[&BString::from("byte_array")].is_byte_array());
            assert!(c[&BString::from("int_array")].is_int_array());
            assert!(c[&BString::from("long_array")].is_long_array());
            assert!(c[&BString::from("list")].is_list());
            assert!(c[&BString::from("compound")].is_compound());
        }};
    }
    for_each_endian!(check);
}

/// Empty `ByteArray`/`IntArray`/`LongArray`/`List`/`Compound` keep distinct
/// tags and re-encode byte-for-byte.
#[test]
fn value_empty_containers_keep_tags() {
    let value = Value::Compound(Compound::from([
        ("byte_array".into(), Value::ByteArray(Vec::new())),
        ("int_array".into(), Value::IntArray(Vec::new())),
        ("long_array".into(), Value::LongArray(Vec::new())),
        ("list".into(), Value::List(ValueList::End)),
        ("compound".into(), Value::Compound(Compound::new())),
    ]));

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&value).unwrap();
            let back: Value = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, value);
            assert_eq!($to(&back).unwrap(), bytes);

            let c = back.as_compound().unwrap();
            assert!(
                c[&BString::from("byte_array")]
                    .as_byte_array()
                    .unwrap()
                    .is_empty()
            );
            assert!(c[&BString::from("int_array")].is_int_array());
            assert!(c[&BString::from("long_array")].is_long_array());
            assert!(c[&BString::from("list")].as_list().unwrap().is_empty());
            assert!(
                c[&BString::from("compound")]
                    .as_compound()
                    .unwrap()
                    .is_empty()
            );
        }};
    }
    for_each_endian!(check);
}

/// Non-UTF-8 string *values* and non-UTF-8 compound *keys* survive losslessly.
#[test]
fn value_non_utf8_roundtrip() {
    let raw_value: BString = BString::from(vec![0x68, 0x69, 0xff, 0xfe, 0x00, 0x80, 0x21]);
    let raw_key: BString = BString::from(vec![0x6b, 0xff, 0xfe, 0x79]);
    assert!(std::str::from_utf8(raw_value.as_ref()).is_err());
    assert!(std::str::from_utf8(raw_key.as_ref()).is_err());

    let value = Value::Compound(Compound::from([
        ("plain".into(), Value::String("Hello, World!".into())),
        (raw_key.clone(), Value::String(raw_value.clone())),
        (
            "nested".into(),
            Value::Compound(Compound::from([(
                raw_key.clone(),
                Value::List(ValueList::String(vec![raw_value.clone(), "mixed".into()])),
            )])),
        ),
    ]));

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&value).unwrap();
            let back: Value = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, value);
            assert_eq!($to(&back).unwrap(), bytes);

            let c = back.as_compound().unwrap();
            assert_eq!(c.get(&raw_key).unwrap().as_string().unwrap(), &raw_value);
        }};
    }
    for_each_endian!(check);
}

/// A single compound mixing all three array tags plus a list must keep them
/// distinct (no cross-contamination) across endiannesses.
#[test]
fn value_mixed_arrays_no_cross_contamination() {
    let value = Value::Compound(Compound::from([
        ("ba".into(), Value::ByteArray(vec![1, 2, 3])),
        ("ia".into(), Value::IntArray(vec![1, 2, 3])),
        ("la".into(), Value::LongArray(vec![1, 2, 3])),
        ("li".into(), Value::List(ValueList::Byte(vec![1, 2, 3]))),
    ]));

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&value).unwrap();
            let back: Value = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, value);

            let c = back.as_compound().unwrap();
            assert!(c[&BString::from("ba")].is_byte_array());
            assert!(c[&BString::from("ia")].is_int_array());
            assert!(c[&BString::from("la")].is_long_array());
            assert!(c[&BString::from("li")].is_list());
        }};
    }
    for_each_endian!(check);
}

/// A heterogeneous list cannot be encoded: the wire format stores one
/// element-type byte for the whole list, so writing mixed elements would desync
/// the stream and silently drop trailing keys.
///
/// `Value::List`'s payload is a [`ValueList`], so the mixture is now refused
/// where the list is *built* — the document below can never be constructed in
/// the first place, and the encoders have nothing left to reject.
#[test]
fn heterogeneous_list_errors_without_dropping_keys() {
    let err = ValueList::try_from(vec![Value::Byte(1), Value::Int(300)]);
    assert!(
        matches!(err, Err(nbtx::Error::HeterogeneousList { .. })),
        "expected HeterogeneousList error, got {err:?}"
    );

    // The document that would have carried it still encodes, once the bad key
    // is left out: nothing about the surrounding compound was at fault.
    let value = Value::Compound(Compound::from([
        ("good".into(), Value::List(ValueList::Byte(vec![1]))),
        ("trailing".into(), Value::Int(7)),
    ]));
    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&value).unwrap();
            let back: Value = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, value);
        }};
    }
    for_each_endian!(check);
}

/// The varint variant encodes ints/longs as varints; the signed boundary values
/// must survive a round-trip (zig-zag varints handle the sign bit).
#[test]
fn varint_boundary_values() {
    let value = Value::Compound(Compound::from([
        ("i_min".into(), Value::Int(i32::MIN)),
        ("i_max".into(), Value::Int(i32::MAX)),
        ("l_min".into(), Value::Long(i64::MIN)),
        ("l_max".into(), Value::Long(i64::MAX)),
    ]));

    let bytes = to_varint_bytes(&value).unwrap();
    let back: Value = from_varint_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(back, value);
}

/// A deeply-nested compound plus a large typed array round-trip without stack or
/// allocation trouble.
#[test]
fn deeply_nested_and_large_array_roundtrip() {
    let mut deep = Value::Byte(1);
    for _ in 0..128 {
        deep = Value::Compound(Compound::from([("n".into(), deep)]));
    }
    let large = Value::IntArray((0..50_000).collect());
    let root = Value::Compound(Compound::from([
        ("deep".into(), deep),
        ("large".into(), large),
    ]));

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&root).unwrap();
            let back: Value = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, root);
        }};
    }
    for_each_endian!(check);
}

/// Malformed and truncated NBT must return `Err`, never panic — including an
/// attacker-controlled huge length prefix (which must not pre-allocate gigabytes).
#[test]
fn malformed_nbt_returns_err_not_panic() {
    // Truncating a valid document at any offset must not panic.
    let value = Value::Compound(Compound::from([
        ("s".into(), Value::String("hello".into())),
        ("a".into(), Value::IntArray(vec![1, 2, 3, 4])),
    ]));
    let bytes = to_be_bytes(&value).unwrap();
    for cut in 0..bytes.len() {
        let res: Result<Value, _> = from_be_bytes(&mut &bytes[..cut]);
        let _ = res; // Only requirement: it does not panic.
    }

    // A bogus root tag type (99) is out of range → Err.
    let res: Result<Value, _> = from_be_bytes(&mut [99u8, 0, 0].as_slice());
    assert!(res.is_err());

    // A ByteArray claiming ~2 GiB with no data behind it must Err promptly
    // (capped preallocation), not OOM.
    let mut malicious = Vec::new();
    malicious.push(10u8); // root compound
    malicious.extend_from_slice(&0u16.to_be_bytes()); // empty root name
    malicious.push(7u8); // ByteArray tag
    malicious.extend_from_slice(&1u16.to_be_bytes()); // key length 1
    malicious.push(b'x'); // key
    malicious.extend_from_slice(&0x7fff_ffffi32.to_be_bytes()); // huge length, no data
    let res: Result<Value, _> = from_be_bytes(&mut malicious.as_slice());
    assert!(res.is_err());
}
