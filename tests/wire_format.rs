//! Byte-exact wire-format vectors for all three NBT variants.
//!
//! Every golden byte string in this file pins the exact output of `to_be_bytes`
//! / `to_le_bytes` / `to_varint_bytes` for a given `Value`, and asserts that
//! those same bytes decode back to it. A wire format is only useful if it is
//! stable, so these are deliberately literal: a change in any of them is a
//! change other implementations will see.
//!
//! The three variants differ only in how they lay out numbers:
//!
//! * big-endian — fixed-width, most significant byte first;
//! * little-endian — fixed-width, least significant byte first;
//! * varint (the Bedrock network variant) — little-endian for Short/Float/
//!   Double, but zigzag varints for Int/Long and array lengths, and an
//!   *unsigned* varint for string lengths.

#![cfg(feature = "nbt")]

use bstr::BString;
use nbtx::{
    Compound, Value, from_be_bytes, from_le_bytes, from_varint_bytes, to_be_bytes, to_le_bytes,
    to_varint_bytes,
};

/// Parses a hex string into bytes. Whitespace is ignored, so vectors may be
/// written either packed (`0a0000`) or spaced (`0a 00 00`).
fn hex(s: &str) -> Vec<u8> {
    let packed: String = s.split_whitespace().collect();
    assert!(packed.len().is_multiple_of(2), "odd-length hex: {s}");
    (0..packed.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&packed[i..i + 2], 16).unwrap())
        .collect()
}

/// Builds a `Value::Compound` in the given key order.
fn comp(entries: &[(&str, Value)]) -> Value {
    let mut m = Compound::new();
    for (k, v) in entries {
        m.insert(BString::from(*k), v.clone());
    }
    Value::Compound(m)
}

/// Asserts `value` encodes to exactly `be`/`le`/`var` in the three variants, and
/// that each of those byte strings decodes back to `value`.
#[track_caller]
fn assert_vector(label: &str, value: &Value, be: &str, le: &str, var: &str) {
    let cases: [(&str, Vec<u8>, Vec<u8>); 3] = [
        ("BigEndian", to_be_bytes(value).unwrap(), hex(be)),
        ("LittleEndian", to_le_bytes(value).unwrap(), hex(le)),
        ("VarintEndian", to_varint_bytes(value).unwrap(), hex(var)),
    ];
    for (variant, got, want) in cases {
        assert_eq!(
            got,
            want,
            "\n{label} / {variant}: encode mismatch\n  got  = {}\n  want = {}",
            got.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            want.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        );
    }

    let rt_be: Value = from_be_bytes(&mut hex(be).as_slice()).unwrap();
    let rt_le: Value = from_le_bytes(&mut hex(le).as_slice()).unwrap();
    let rt_var: Value = from_varint_bytes(&mut hex(var).as_slice()).unwrap();
    for (variant, rt) in [
        ("BigEndian", rt_be),
        ("LittleEndian", rt_le),
        ("VarintEndian", rt_var),
    ] {
        assert_eq!(&rt, value, "{label} / {variant}: decode mismatch");
    }
}

#[test]
fn byte_tag_encodes_exactly() {
    assert_vector(
        "byte",
        &Value::Byte(5),
        "01 00 00 05",
        "01 00 00 05",
        "01 00 05",
    );
    assert_vector(
        "byte_neg",
        &Value::Byte(-1),
        "01 00 00 ff",
        "01 00 00 ff",
        "01 00 ff",
    );
}

#[test]
fn short_tag_encodes_exactly() {
    assert_vector(
        "short",
        &Value::Short(300),
        "02 00 00 01 2c",
        "02 00 00 2c 01",
        "02 00 2c 01",
    );
    assert_vector(
        "short_neg",
        &Value::Short(-300),
        "02 00 00 fe d4",
        "02 00 00 d4 fe",
        "02 00 d4 fe",
    );
}

#[test]
fn int_tag_encodes_exactly_including_zigzag_boundaries() {
    assert_vector(
        "int",
        &Value::Int(5),
        "03 00 00 00 00 00 05",
        "03 00 00 05 00 00 00",
        "03 00 0a",
    );
    assert_vector(
        "int_neg",
        &Value::Int(-1),
        "03 00 00 ff ff ff ff",
        "03 00 00 ff ff ff ff",
        "03 00 01",
    );
    // zigzag(i32::MIN) = 0xFFFFFFFF -> a full 5-byte varint.
    assert_vector(
        "int_min",
        &Value::Int(i32::MIN),
        "03 00 00 80 00 00 00",
        "03 00 00 00 00 00 80",
        "03 00 ff ff ff ff 0f",
    );
    // zigzag(i32::MAX) = 0xFFFFFFFE.
    assert_vector(
        "int_max",
        &Value::Int(i32::MAX),
        "03 00 00 7f ff ff ff",
        "03 00 00 ff ff ff 7f",
        "03 00 fe ff ff ff 0f",
    );
}

#[test]
fn long_tag_encodes_exactly_including_zigzag_boundaries() {
    assert_vector(
        "long",
        &Value::Long(5),
        "04 00 00 00 00 00 00 00 00 00 05",
        "04 00 00 05 00 00 00 00 00 00 00",
        "04 00 0a",
    );
    assert_vector(
        "long_neg",
        &Value::Long(-1),
        "04 00 00 ff ff ff ff ff ff ff ff",
        "04 00 00 ff ff ff ff ff ff ff ff",
        "04 00 01",
    );
    assert_vector(
        "long_min",
        &Value::Long(i64::MIN),
        "04 00 00 80 00 00 00 00 00 00 00",
        "04 00 00 00 00 00 00 00 00 00 80",
        "04 00 ff ff ff ff ff ff ff ff ff 01",
    );
    assert_vector(
        "long_max",
        &Value::Long(i64::MAX),
        "04 00 00 7f ff ff ff ff ff ff ff",
        "04 00 00 ff ff ff ff ff ff ff 7f",
        "04 00 fe ff ff ff ff ff ff ff ff 01",
    );
}

#[test]
fn float_and_double_tags_encode_exactly() {
    // Float/Double stay fixed-width little-endian in the varint variant; only
    // Int/Long and the length prefixes become varints.
    assert_vector(
        "float",
        &Value::Float(1.5),
        "05 00 00 3f c0 00 00",
        "05 00 00 00 00 c0 3f",
        "05 00 00 00 c0 3f",
    );
    assert_vector(
        "double",
        &Value::Double(1.5),
        "06 00 00 3f f8 00 00 00 00 00 00",
        "06 00 00 00 00 00 00 00 00 f8 3f",
        "06 00 00 00 00 00 00 00 f8 3f",
    );
}

#[test]
fn string_tag_encodes_exactly() {
    // BE/LE use a u16 length prefix; the varint variant uses an unsigned
    // varint, not a zigzag one (lengths are never negative).
    assert_vector(
        "string",
        &Value::from("hi"),
        "08 00 00 00 02 68 69",
        "08 00 00 02 00 68 69",
        "08 00 02 68 69",
    );
    assert_vector(
        "string_empty",
        &Value::from(""),
        "08 00 00 00 00",
        "08 00 00 00 00",
        "08 00 00",
    );
}

#[test]
fn byte_array_tag_encodes_exactly() {
    // Array lengths are a signed i32 in BE/LE and a zigzag varint in the
    // varint variant.
    assert_vector(
        "bytearray",
        &Value::ByteArray(vec![1, 2, 3]),
        "07 00 00 00 00 00 03 01 02 03",
        "07 00 00 03 00 00 00 01 02 03",
        "07 00 06 01 02 03", // 06 = zigzag(3)
    );
    assert_vector(
        "bytearray_empty",
        &Value::ByteArray(vec![]),
        "07 00 00 00 00 00 00",
        "07 00 00 00 00 00 00",
        "07 00 00",
    );
}

#[test]
fn int_array_tag_encodes_exactly() {
    assert_vector(
        "intarray",
        &Value::IntArray(vec![1, -1]),
        "0b 00 00 00 00 00 02 00 00 00 01 ff ff ff ff",
        "0b 00 00 02 00 00 00 01 00 00 00 ff ff ff ff",
        "0b 00 04 02 01", // len zigzag(2)=04, then zigzag(1)=02, zigzag(-1)=01
    );
    assert_vector(
        "intarray_empty",
        &Value::IntArray(vec![]),
        "0b 00 00 00 00 00 00",
        "0b 00 00 00 00 00 00",
        "0b 00 00",
    );
}

#[test]
fn long_array_tag_encodes_exactly() {
    assert_vector(
        "longarray",
        &Value::LongArray(vec![1, -1]),
        "0c 00 00 00 00 00 02 00 00 00 00 00 00 00 01 ff ff ff ff ff ff ff ff",
        "0c 00 00 02 00 00 00 01 00 00 00 00 00 00 00 ff ff ff ff ff ff ff ff",
        "0c 00 04 02 01",
    );
    assert_vector(
        "longarray_empty",
        &Value::LongArray(vec![]),
        "0c 00 00 00 00 00 00",
        "0c 00 00 00 00 00 00",
        "0c 00 00",
    );
}

#[test]
fn list_of_shorts_encodes_exactly() {
    // A list is one element-type byte, then the length, then bare payloads with
    // no per-element tag or name.
    assert_vector(
        "list_short",
        &Value::List(vec![Value::Short(1), Value::Short(2), Value::Short(3)]),
        "09 00 00 02 00 00 00 03 00 01 00 02 00 03",
        "09 00 00 02 03 00 00 00 01 00 02 00 03 00",
        "09 00 02 06 01 00 02 00 03 00", // elem=02, len zigzag(3)=06
    );
}

#[test]
fn empty_list_encodes_a_tag_end_element_type() {
    assert_vector(
        "list_empty",
        &Value::List(vec![]),
        "09 00 00 00 00 00 00 00",
        "09 00 00 00 00 00 00 00",
        "09 00 00 00",
    );
}

#[test]
fn list_of_strings_encodes_exactly() {
    assert_vector(
        "list_string",
        &Value::List(vec![Value::from("a"), Value::from("bb")]),
        "09 00 00 08 00 00 00 02 00 01 61 00 02 62 62",
        "09 00 00 08 02 00 00 00 01 00 61 02 00 62 62",
        "09 00 08 04 01 61 02 62 62",
    );
}

#[test]
fn list_of_compounds_encodes_exactly() {
    assert_vector(
        "list_compound",
        &Value::List(vec![comp(&[("k", Value::Byte(1))])]),
        "09 00 00 0a 00 00 00 01 01 00 01 6b 01 00",
        "09 00 00 0a 01 00 00 00 01 01 00 6b 01 00",
        "09 00 0a 02 01 01 6b 01 00",
    );
}

#[test]
fn single_key_compound_encodes_exactly() {
    assert_vector(
        "compound_single",
        &comp(&[("k", Value::Int(7))]),
        "0a 00 00 03 00 01 6b 00 00 00 07 00",
        "0a 00 00 03 01 00 6b 07 00 00 00 00",
        "0a 00 03 01 6b 0e 00", // 0e = zigzag(7)
    );
}

#[test]
fn multi_key_compound_encodes_in_insertion_order() {
    assert_vector(
        "compound_multi",
        &comp(&[
            ("a", Value::Byte(1)),
            ("b", Value::Short(2)),
            ("c", Value::from("x")),
        ]),
        "0a 00 00 01 00 01 61 01 02 00 01 62 00 02 08 00 01 63 00 01 78 00",
        "0a 00 00 01 01 00 61 01 02 01 00 62 02 00 08 01 00 63 01 00 78 00",
        "0a 00 01 01 61 01 02 01 62 02 00 08 01 63 01 78 00",
    );
}

#[test]
fn nested_compound_encodes_exactly() {
    assert_vector(
        "nested_val",
        &comp(&[("inner", comp(&[("x", Value::Int(9))]))]),
        "0a 00 00 0a 00 05 69 6e 6e 65 72 03 00 01 78 00 00 00 09 00 00",
        "0a 00 00 0a 05 00 69 6e 6e 65 72 03 01 00 78 09 00 00 00 00 00",
        "0a 00 0a 05 69 6e 6e 65 72 03 01 78 12 00 00", // 12 = zigzag(9)
    );
}

#[test]
fn non_utf8_string_in_a_compound_encodes_as_raw_bytes() {
    // NBT strings are length-prefixed raw bytes: no modified-UTF-8, no
    // validation, so arbitrary byte sequences survive.
    assert_vector(
        "nonutf8",
        &comp(&[("s", Value::String(BString::from(vec![0xffu8, 0xfe])))]),
        "0a 00 00 08 00 01 73 00 02 ff fe 00",
        "0a 00 00 08 01 00 73 02 00 ff fe 00",
        "0a 00 08 01 73 02 ff fe 00",
    );
}

#[test]
fn single_key_compound_decodes_and_reencodes() {
    assert_vector(
        "roundtrip_single",
        &comp(&[("k", Value::Int(7))]),
        "0a 00 00 03 00 01 6b 00 00 00 07 00",
        "0a 00 00 03 01 00 6b 07 00 00 00 00",
        "0a 00 03 01 6b 0e 00",
    );
}

#[test]
fn mixed_tag_compound_decodes_and_reencodes() {
    // One compound holding a ByteArray, an IntArray, a LongArray, a List and a
    // String — the tags most easily confused with one another.
    let v = comp(&[
        ("ba", Value::ByteArray(vec![9, 8])),
        ("ia", Value::IntArray(vec![-5])),
        ("la", Value::LongArray(vec![7])),
        ("li", Value::List(vec![Value::Short(1), Value::Short(2)])),
        ("s", Value::from("ok")),
    ]);
    assert_vector(
        "kitchen_sink",
        &v,
        "0a 00 00 07 00 02 62 61 00 00 00 02 09 08 0b 00 02 69 61 00 00 00 01 ff ff ff fb 0c 00 02 6c 61 00 00 00 01 00 00 00 00 00 00 00 07 09 00 02 6c 69 02 00 00 00 02 00 01 00 02 08 00 01 73 00 02 6f 6b 00",
        "0a 00 00 07 02 00 62 61 02 00 00 00 09 08 0b 02 00 69 61 01 00 00 00 fb ff ff ff 0c 02 00 6c 61 01 00 00 00 07 00 00 00 00 00 00 00 09 02 00 6c 69 02 02 00 00 00 01 00 02 00 08 01 00 73 02 00 6f 6b 00",
        "0a 00 07 02 62 61 04 09 08 0b 02 69 61 02 09 0c 02 6c 61 02 0e 09 02 6c 69 02 04 01 00 02 00 08 01 73 02 6f 6b 00",
    );
}

#[test]
fn scalar_tags_in_a_compound_encode_exactly() {
    assert_vector(
        "byte 1",
        &comp(&[("a", Value::Byte(1))]),
        "0a0000010001610100",
        "0a0000010100610100",
        "0a000101610100",
    );
    assert_vector(
        "short -2",
        &comp(&[("a", Value::Short(-2))]),
        "0a000002000161fffe00",
        "0a000002010061feff00",
        "0a00020161feff00",
    );
    assert_vector(
        "float 1.5",
        &comp(&[("a", Value::Float(1.5))]),
        "0a0000050001613fc0000000",
        "0a0000050100610000c03f00",
        "0a000501610000c03f00",
    );
    assert_vector(
        "double 1.5",
        &comp(&[("a", Value::Double(1.5))]),
        "0a0000060001613ff800000000000000",
        "0a000006010061000000000000f83f00",
        "0a00060161000000000000f83f00",
    );
}

/// Int and Long become **zigzag** varints in the varint variant, so small
/// negative values stay one byte instead of costing the full width.
#[test]
fn varint_variant_uses_zigzag_for_int_and_long() {
    assert_vector(
        "int -2",
        &comp(&[("a", Value::Int(-2))]),
        "0a000003000161fffffffe00",
        "0a000003010061feffffff00",
        "0a000301610300", // zigzag(-2) = 3
    );
    assert_vector(
        "int MAX",
        &comp(&[("a", Value::Int(i32::MAX))]),
        "0a0000030001617fffffff00",
        "0a000003010061ffffff7f00",
        "0a00030161feffffff0f00",
    );
    assert_vector(
        "int MIN",
        &comp(&[("a", Value::Int(i32::MIN))]),
        "0a0000030001618000000000",
        "0a0000030100610000008000",
        "0a00030161ffffffff0f00",
    );
    assert_vector(
        "long -2",
        &comp(&[("a", Value::Long(-2))]),
        "0a000004000161fffffffffffffffe00",
        "0a000004010061feffffffffffffff00",
        "0a000401610300",
    );
    assert_vector(
        "long MIN",
        &comp(&[("a", Value::Long(i64::MIN))]),
        "0a000004000161800000000000000000",
        "0a000004010061000000000000008000",
        "0a00040161ffffffffffffffffff0100",
    );
}

/// String lengths are **unsigned** varints — a length can never be negative, so
/// zigzagging it would waste the sign bit and double the byte cost of short
/// strings.
#[test]
fn varint_variant_string_length_is_unsigned() {
    assert_vector(
        "string \"hi\"",
        &comp(&[("a", Value::String("hi".into()))]),
        "0a0000080001610002686900",
        "0a0000080100610200686900",
        "0a0008016102686900", // len 2 -> varuint 0x02 (zigzag would be 0x04)
    );
    assert_vector(
        "string \"\"",
        &comp(&[("a", Value::String("".into()))]),
        "0a000008000161000000",
        "0a000008010061000000",
        "0a000801610000",
    );
}

/// ByteArray/IntArray/LongArray length prefixes go through the same signed-i32
/// path as any other Int, so they are zigzagged in the varint variant.
#[test]
fn typed_arrays_encode_exactly() {
    assert_vector(
        "ByteArray [1,2,3]",
        &comp(&[("a", Value::ByteArray(vec![1, 2, 3]))]),
        "0a0000070001610000000301020300",
        "0a0000070100610300000001020300",
        "0a000701610601020300", // zigzag(3) = 6
    );
    assert_vector(
        "IntArray [-1,300]",
        &comp(&[("a", Value::IntArray(vec![-1, 300]))]),
        "0a00000b00016100000002ffffffff0000012c00",
        "0a00000b01006102000000ffffffff2c01000000",
        "0a000b01610401d80400",
    );
    assert_vector(
        "LongArray [-1,300]",
        &comp(&[("a", Value::LongArray(vec![-1, 300]))]),
        "0a00000c00016100000002ffffffffffffffff000000000000012c00",
        "0a00000c01006102000000ffffffffffffffff2c0100000000000000",
        "0a000c01610401d80400",
    );
    assert_vector(
        "ByteArray []",
        &comp(&[("a", Value::ByteArray(vec![]))]),
        "0a0000070001610000000000",
        "0a0000070100610000000000",
        "0a000701610000",
    );
    assert_vector(
        "IntArray []",
        &comp(&[("a", Value::IntArray(vec![]))]),
        "0a00000b0001610000000000",
        "0a00000b0100610000000000",
        "0a000b01610000",
    );
}

/// TAG_IntArray (11) and TAG_List (9) of Ints share a payload layout but are
/// different tags, so a decoder cannot recover one from the other. `Value` keeps
/// them apart end to end.
#[test]
fn int_array_and_list_of_int_are_distinct_tags() {
    assert_vector(
        "List<Int> [-1,300]",
        &comp(&[("a", Value::List(vec![Value::Int(-1), Value::Int(300)]))]),
        "0a0000090001610300000002ffffffff0000012c00",
        "0a0000090100610302000000ffffffff2c01000000",
        "0a00090161030401d80400",
    );
    assert_vector(
        "List<Long> [-1,300]",
        &comp(&[("a", Value::List(vec![Value::Long(-1), Value::Long(300)]))]),
        "0a0000090001610400000002ffffffffffffffff000000000000012c00",
        "0a0000090100610402000000ffffffffffffffff2c0100000000000000",
        "0a00090161040401d80400",
    );

    // The IntArray payload after the tag byte is byte-identical to the
    // List<Int> payload after the element-type byte — only the tag differs.
    let ia = to_be_bytes(&comp(&[("a", Value::IntArray(vec![-1, 300]))])).unwrap();
    let li = to_be_bytes(&comp(&[(
        "a",
        Value::List(vec![Value::Int(-1), Value::Int(300)]),
    )]))
    .unwrap();
    assert_eq!(ia[3], 0x0b, "IntArray must use tag 11");
    assert_eq!(li[3], 0x09, "List must use tag 9");
    assert_eq!(li[7], 0x03, "List<Int> element type must be tag 3");
    assert_eq!(&ia[7..], &li[8..], "payloads must be identical");
}

#[test]
fn lists_encode_exactly() {
    assert_vector(
        "List<Short> [1,2]",
        &comp(&[("a", Value::List(vec![Value::Short(1), Value::Short(2)]))]),
        "0a00000900016102000000020001000200",
        "0a00000901006102020000000100020000",
        "0a0009016102040100020000",
    );
    assert_vector(
        "List<String> [x,yy]",
        &comp(&[(
            "a",
            Value::List(vec![Value::String("x".into()), Value::String("yy".into())]),
        )]),
        "0a00000900016108000000020001780002797900",
        "0a00000901006108020000000100780200797900",
        "0a000901610804017802797900",
    );
    assert_vector(
        "List<Byte> [1,2,3]",
        &comp(&[(
            "a",
            Value::List(vec![Value::Byte(1), Value::Byte(2), Value::Byte(3)]),
        )]),
        "0a000009000161010000000301020300",
        "0a000009010061010300000001020300",
        "0a00090161010601020300",
    );
    assert_vector(
        "List<Compound> [{b:7b}]",
        &comp(&[("a", Value::List(vec![comp(&[("b", Value::Byte(7))])]))]),
        "0a0000090001610a0000000101000162070000",
        "0a0000090100610a0100000001010062070000",
        "0a000901610a02010162070000",
    );
}

/// An empty list has no element to take a type from, so the canonical encoding
/// declares TAG_End (0).
#[test]
fn empty_list_element_type_is_tag_end() {
    assert_vector(
        "List []",
        &comp(&[("a", Value::List(vec![]))]),
        "0a000009000161000000000000",
        "0a000009010061000000000000",
        "0a00090161000000",
    );
}

#[test]
fn nested_and_empty_compounds_encode_exactly() {
    assert_vector(
        "nested compound",
        &comp(&[("a", comp(&[("b", Value::Int(5))]))]),
        "0a00000a00016103000162000000050000",
        "0a00000a01006103010062050000000000",
        "0a000a01610301620a0000",
    );
    assert_vector(
        "empty root compound",
        &comp(&[]),
        "0a000000",
        "0a000000",
        "0a0000",
    );
}

#[test]
fn non_utf8_string_payload_passes_through() {
    assert_vector(
        "string \\xff\\xfe",
        &comp(&[("a", Value::String(vec![0xff, 0xfe].into()))]),
        "0a0000080001610002fffe00",
        "0a0000080100610200fffe00",
        "0a0008016102fffe00",
    );
}

/// Compound *keys* use the same String codec as payloads, so they are raw bytes
/// too and a non-UTF-8 key survives a round-trip.
#[test]
fn non_utf8_compound_key_roundtrips() {
    // root compound, empty name; key = raw bytes ff fe; Byte 7; end
    let bytes = hex(concat!("0a0000", "01", "0200", "fffe", "07", "00"));
    let v: Value = from_le_bytes(&mut bytes.as_slice()).unwrap();
    let Value::Compound(m) = &v else {
        panic!("expected compound")
    };
    assert_eq!(m.len(), 1);
    let (k, val) = m.iter().next().unwrap();
    assert_eq!(k.as_slice(), &[0xff, 0xfe]);
    assert!(matches!(val, Value::Byte(7)));
    assert_eq!(to_le_bytes(&v).unwrap(), bytes);
}
