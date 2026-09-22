//! Per-tag and per-container semantics: what each NBT tag means, which tags are
//! genuinely distinct from one another, and the rules the compound and list
//! containers follow.
//!
//! These are behavioural rather than byte-exact — the exact bytes are pinned in
//! `wire_format.rs`.

#![cfg(feature = "nbt")]

use bstr::BString;
use nbtx::{
    Compound, Value, ValueList, from_be_bytes, from_le_bytes, from_varint_bytes, to_be_bytes,
    to_le_bytes, to_varint_bytes,
};

fn compound(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Compound(
        entries
            .into_iter()
            .map(|(k, v)| (BString::from(k), v))
            .collect::<Compound>(),
    )
}

fn root(key: &str, v: Value) -> Value {
    Value::Compound(Compound::from([(BString::from(key), v)]))
}

fn get<'a>(v: &'a Value, key: &str) -> &'a Value {
    v.as_compound().unwrap().get(&BString::from(key)).unwrap()
}

fn hex(s: &str) -> Vec<u8> {
    let packed: String = s.split_whitespace().collect();
    assert!(packed.len().is_multiple_of(2), "odd-length hex: {s}");
    (0..packed.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&packed[i..i + 2], 16).unwrap())
        .collect()
}

/// Hand-assembles big-endian documents, so tests can craft inputs the encoder
/// would never produce (duplicate keys, empty typed lists, ...).
mod be {
    pub fn str_payload(out: &mut Vec<u8>, s: &[u8]) {
        out.extend_from_slice(&(s.len() as u16).to_be_bytes());
        out.extend_from_slice(s);
    }
    /// Begins a root compound with an empty name.
    pub fn root_compound() -> Vec<u8> {
        let mut v = vec![10u8];
        str_payload(&mut v, b"");
        v
    }
    pub fn entry_header(out: &mut Vec<u8>, tag: u8, key: &[u8]) {
        out.push(tag);
        str_payload(out, key);
    }
    pub fn end(out: &mut Vec<u8>) {
        out.push(0);
    }
}

/// Invokes a caller-defined `check!($to, $from)` macro once for each of the
/// three endianness function pairs.
macro_rules! for_each_endian {
    ($check:ident) => {
        $check!(to_be_bytes, from_be_bytes);
        $check!(to_le_bytes, from_le_bytes);
        $check!(to_varint_bytes, from_varint_bytes);
    };
}

/// The tag ids are part of the wire format and can never be renumbered.
#[test]
fn tag_ids_are_fixed() {
    use nbtx::FieldType::*;
    for (tag, id) in [
        (End, 0u8),
        (Byte, 1),
        (Short, 2),
        (Int, 3),
        (Long, 4),
        (Float, 5),
        (Double, 6),
        (ByteArray, 7),
        (String, 8),
        (List, 9),
        (Compound, 10),
        (IntArray, 11),
        (LongArray, 12),
    ] {
        assert_eq!(tag as u8, id, "tag id mismatch for {tag}");
    }
}

/// One of every tag except `LongArray` (covered by the next test), written and
/// read back, in all three variants.
#[test]
fn every_tag_type_roundtrips_in_all_variants() {
    let doc = compound([
        ("byte", Value::Byte(1)),
        ("short", Value::Short(1)),
        ("int", Value::Int(1)),
        ("long", Value::Long(1)),
        ("float", Value::Float(1.0)),
        ("double", Value::Double(1.0)),
        ("bytearray", Value::ByteArray(vec![1])),
        ("string", Value::String(BString::from("string"))),
        ("list", Value::List(ValueList::Byte(vec![1]))),
        ("intarray", Value::IntArray(vec![1])),
    ]);

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&doc).unwrap();
            let back: Value = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(
                back, doc,
                concat!("round-trip failed for ", stringify!($to))
            );
        }};
    }
    for_each_endian!(check);
}

/// The same including `LongArray` (tag 12), and additionally asserting the
/// re-encode is byte-identical. Keys are in sorted order so this holds under
/// both `Compound` backings (`IndexMap` by default, `BTreeMap` without
/// `preserve_order`).
#[test]
fn every_tag_type_including_long_array_reencodes_identically() {
    let value = Value::Compound(Compound::from([
        (BString::from("a_byte"), Value::Byte(1)),
        (BString::from("b_short"), Value::Short(1)),
        (BString::from("c_int"), Value::Int(1)),
        (BString::from("d_long"), Value::Long(1)),
        (BString::from("e_float"), Value::Float(1.0)),
        (BString::from("f_double"), Value::Double(1.0)),
        (BString::from("g_bytearray"), Value::ByteArray(vec![1])),
        (BString::from("h_string"), Value::String("string".into())),
        (
            BString::from("i_list"),
            Value::List(ValueList::Byte(vec![1])),
        ),
        (BString::from("j_intarray"), Value::IntArray(vec![1])),
        (BString::from("k_longarray"), Value::LongArray(vec![1])),
    ]));

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&value).unwrap();
            let back: Value = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, value, "value equality after decode");
            assert_eq!($to(&back).unwrap(), bytes, "byte-identical re-encode");
        }};
    }
    for_each_endian!(check);
}

/// The integer tags' ranges are encoded in the Rust type system
/// (`Value::Byte(i8)`, `Short(i16)`, `Int(i32)`, `Long(i64)`), which makes an
/// out-of-range value a compile error rather than a runtime one. This is the
/// runtime-observable half: every boundary value survives a round-trip.
#[test]
fn integer_tag_boundaries_roundtrip() {
    let value = Value::Compound(Compound::from([
        (BString::from("a"), Value::Byte(i8::MIN)),
        (BString::from("b"), Value::Byte(i8::MAX)),
        (BString::from("c"), Value::Short(i16::MIN)),
        (BString::from("d"), Value::Short(i16::MAX)),
        (BString::from("e"), Value::Int(i32::MIN)),
        (BString::from("f"), Value::Int(i32::MAX)),
        (BString::from("g"), Value::Long(i64::MIN)),
        (BString::from("h"), Value::Long(i64::MAX)),
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

#[test]
fn string_value_roundtrip() {
    let s = "the quick brown fox jumped over the lazy dog";
    let value = root("string", Value::String(s.into()));
    let bytes = to_be_bytes(&value).unwrap();
    let back: Value = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(get(&back, "string"), &Value::String(s.into()));
}

/// NBT strings are raw bytes with no UTF-8 validation, so arbitrary bytes
/// survive as both a value and a compound key. `Value` stores them as `BString`
/// precisely so this is lossless.
#[test]
fn strings_are_raw_bytes_with_no_utf8_validation() {
    let raw = BString::from(vec![0xffu8, 0xfe, 0x00, 0x80, b'a']);
    let doc = Value::Compound(Compound::from([(raw.clone(), Value::String(raw.clone()))]));
    let bytes = to_le_bytes(&doc).unwrap();
    let back: Value = from_le_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(back, doc, "non-UTF-8 key/value must round-trip losslessly");
}

/// Arrays large enough to exercise the bulk read/write paths rather than the
/// single-element ones.
#[test]
fn array_tags_value_roundtrip() {
    let bytes_payload: Vec<u8> = (0..8192).map(|i| (i % 256) as u8).collect();
    let ints_payload: Vec<i32> = vec![1_234_567; 256];
    let value = Value::Compound(Compound::from([
        (BString::from("a"), Value::ByteArray(bytes_payload.clone())),
        (BString::from("b"), Value::IntArray(ints_payload.clone())),
    ]));

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let enc = $to(&value).unwrap();
            let back: Value = $from(&mut enc.as_slice()).unwrap();
            assert_eq!(get(&back, "a"), &Value::ByteArray(bytes_payload.clone()));
            assert_eq!(get(&back, "b"), &Value::IntArray(ints_payload.clone()));
        }};
    }
    for_each_endian!(check);
}

/// `Value::Float` is an `f32`, so no precision is gained or lost on the way in
/// and the exact bit pattern comes back out.
#[test]
fn float_equality_after_decode() {
    for &f in &[0.3_f32, f32::EPSILON, f32::MAX, f32::MIN_POSITIVE] {
        let value = root("f", Value::Float(f));
        let bytes = to_le_bytes(&value).unwrap();
        let back: Value = from_le_bytes(&mut bytes.as_slice()).unwrap();
        assert_eq!(get(&back, "f"), &Value::Float(f));
    }
}

/// Doubles, including infinity and NaN, round-trip bit-for-bit — the codec must
/// not normalise them.
#[test]
fn double_value_roundtrip() {
    for &d in &[0.3_f64, f64::MAX, f64::MIN_POSITIVE, f64::INFINITY] {
        let value = root("d", Value::Double(d));
        let bytes = to_le_bytes(&value).unwrap();
        let back: Value = from_le_bytes(&mut bytes.as_slice()).unwrap();
        assert_eq!(get(&back, "d").as_double().unwrap(), &d);
    }
    // NaN: not equal to itself, so compare the property instead.
    let value = root("d", Value::Double(f64::NAN));
    let bytes = to_le_bytes(&value).unwrap();
    let back: Value = from_le_bytes(&mut bytes.as_slice()).unwrap();
    assert!(get(&back, "d").as_double().unwrap().is_nan());
}

#[test]
fn list_homogeneous_roundtrip() {
    let list = Value::List(ValueList::String(vec![
        "test0".into(),
        "test1".into(),
        "test2".into(),
    ]));
    let value = root("list", list.clone());
    let bytes = to_be_bytes(&value).unwrap();
    let back: Value = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(get(&back, "list"), &list);
    assert_eq!(get(&back, "list").as_list().unwrap().len(), 3);
}

/// A list carries one element-type byte for all of its elements, so a
/// heterogeneous list has no valid encoding at all. It is rejected as the list
/// is built, rather than silently written in a form that would desync the
/// reader — so a document holding one can never be handed to an encoder.
#[test]
fn list_heterogeneous_is_rejected_when_the_list_is_built() {
    let res = ValueList::try_from(vec![Value::Byte(1), Value::Int(300)]);
    assert!(
        matches!(res, Err(nbtx::Error::HeterogeneousList { .. })),
        "expected HeterogeneousList, got {res:?}"
    );
    // Pushing the odd element onto a typed list is the same refusal.
    let mut list = ValueList::Byte(vec![1]);
    assert!(
        matches!(
            list.push(Value::Int(300)),
            Err(nbtx::Error::HeterogeneousList { .. })
        ),
        "push must refuse a foreign tag"
    );
    // ...and the untouched list still encodes.
    let value = root("list", Value::List(list));
    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&value).unwrap();
            let back: Value = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, value);
        }};
    }
    for_each_endian!(check);
}

/// TAG_List (9) of Ints and TAG_IntArray (11) are separate tags with separate
/// meanings, and `Value` must not conflate them in either direction.
#[test]
fn list_of_int_is_distinct_from_int_array() {
    let list = compound([("v", Value::List(ValueList::Int(vec![1, 2])))]);
    let array = compound([("v", Value::IntArray(vec![1, 2]))]);

    let lb = to_be_bytes(&list).unwrap();
    let ab = to_be_bytes(&array).unwrap();
    assert_ne!(lb, ab, "list-of-int and int-array must not encode alike");
    // Layout: [10][u16 root-name len = 0][entry tag byte]...
    assert_eq!(lb[3], 9, "expected TAG_List (9) tag byte");
    assert_eq!(ab[3], 11, "expected TAG_IntArray (11) tag byte");

    assert_eq!(from_be_bytes::<Value>(&mut lb.as_slice()).unwrap(), list);
    assert_eq!(from_be_bytes::<Value>(&mut ab.as_slice()).unwrap(), array);
}

/// An *empty* typed list keeps the element type the wire declared.
///
/// `Value::List` carries a [`ValueList`], which names an element type whether
/// or not it has any elements, so an empty `List<Byte>` decodes as
/// `ValueList::Byte(vec![])` and re-encodes with element-type byte 1 —
/// byte-for-byte the input. Before 4.0 the payload was a `Vec<Value>`, which
/// had nowhere to put that byte and normalised every empty list to `TAG_End`;
/// that divergence from pmmp/NBT and gophertunnel is what the typed payload
/// removes.
#[test]
fn empty_typed_list_preserves_element_type() {
    let mut buf = be::root_compound();
    be::entry_header(&mut buf, 9, b"list"); // TAG_List
    buf.push(1); // element type TAG_Byte
    buf.extend_from_slice(&0i32.to_be_bytes()); // length 0
    be::end(&mut buf);

    let back: Value = from_be_bytes(&mut buf.as_slice()).unwrap();
    assert_eq!(
        get(&back, "list"),
        &Value::List(ValueList::Byte(Vec::new()))
    );

    let re = to_be_bytes(&back).unwrap();
    assert_eq!(
        re, buf,
        "empty list's element type (TAG_Byte) must survive a round-trip"
    );
}

/// `TAG_End` as an element type is still legal — for a list that really is
/// untyped — and it stays distinct from an empty typed list in both directions.
#[test]
fn an_empty_end_list_is_distinct_from_an_empty_typed_list() {
    let mut buf = be::root_compound();
    be::entry_header(&mut buf, 9, b"list");
    buf.push(0); // element type TAG_End
    buf.extend_from_slice(&0i32.to_be_bytes()); // length 0
    be::end(&mut buf);

    let back: Value = from_be_bytes(&mut buf.as_slice()).unwrap();
    assert_eq!(get(&back, "list"), &Value::List(ValueList::End));
    assert_ne!(
        get(&back, "list"),
        &Value::List(ValueList::Byte(Vec::new())),
        "element type 0 and element type 1 are different values"
    );
    assert_eq!(to_be_bytes(&back).unwrap(), buf);
}

/// Compound keys are strings, never numbers, so a numeric-looking key stays a
/// key rather than becoming an array index.
#[test]
fn compound_numeric_string_keys_roundtrip() {
    let mut map = Compound::new();
    for i in 0..10 {
        map.insert(
            BString::from(i.to_string()),
            Value::String(i.to_string().into()),
        );
    }
    let value = Value::Compound(map);
    let bytes = to_be_bytes(&value).unwrap();
    let back: Value = from_be_bytes(&mut bytes.as_slice()).unwrap();
    for i in 0..10 {
        assert_eq!(
            back.as_compound()
                .unwrap()
                .get(&BString::from(i.to_string()))
                .unwrap(),
            &Value::String(i.to_string().into())
        );
    }
}

/// Every inserted key is present after a round-trip, and the count matches.
/// (Iteration *order* is asserted in `key_order_is_preserved`, which is
/// feature-dependent.)
#[test]
fn compound_all_keys_present_after_roundtrip() {
    let mut map = Compound::new();
    for i in 0..10 {
        map.insert(
            BString::from(format!("hello{i}")),
            Value::String(i.to_string().into()),
        );
    }
    let value = Value::Compound(map);
    let bytes = to_be_bytes(&value).unwrap();
    let back: Value = from_be_bytes(&mut bytes.as_slice()).unwrap();
    let c = back.as_compound().unwrap();
    assert_eq!(c.len(), 10);
    for i in 0..10 {
        assert!(c.contains_key(&BString::from(format!("hello{i}"))));
    }
}

/// Inserting the same key twice *in memory* replaces the value, as any map
/// does. This is the opposite of the on-wire rule below, which is about parsing
/// a document that already contains the key twice.
#[test]
fn compound_later_key_overwrites_in_memory() {
    let mut map = Compound::new();
    map.insert(BString::from("test1"), Value::String("test1".into()));
    map.insert(BString::from("test2"), Value::Int(2));
    map.insert(BString::from("test1"), Value::String("replacement".into()));
    assert_eq!(map.len(), 2);
    assert_eq!(
        map.get(&BString::from("test1")).unwrap(),
        &Value::String("replacement".into())
    );
    assert_eq!(map.get(&BString::from("test2")).unwrap(), &Value::Int(2));
}

/// A key repeated **on the wire** keeps its first value and its original
/// position; later duplicates are still fully parsed, so the stream stays in
/// sync, but are dropped.
///
/// Duplicate keys are a known corruption pattern in old world saves, and the
/// first value is the one the rest of the document was written against — so
/// first-wins is the recovery that preserves the document's internal
/// consistency. The naive implementation (insert into a map) gives last-wins
/// instead; nbtx deliberately does not.
#[test]
fn duplicate_compound_keys_keep_the_first_value() {
    let mut buf = be::root_compound();
    for v in [1i32, 2i32] {
        be::entry_header(&mut buf, 3, b"d"); // TAG_Int, key "d"
        buf.extend_from_slice(&v.to_be_bytes());
    }
    be::end(&mut buf);
    let back: Value = from_be_bytes(&mut buf.as_slice()).unwrap();
    assert_eq!(
        get(&back, "d"),
        &Value::Int(1),
        "the first value for a duplicated key must win"
    );
}

/// The dropped duplicate must not leave a second entry behind, and must not
/// disturb the rest of the stream.
#[test]
fn duplicate_compound_keys_collapse_to_one_entry() {
    let mut buf = be::root_compound();
    for v in [1i32, 2i32] {
        be::entry_header(&mut buf, 3, b"d");
        buf.extend_from_slice(&v.to_be_bytes());
    }
    be::end(&mut buf);
    let back: Value = from_be_bytes(&mut buf.as_slice()).unwrap();
    assert_eq!(get(&back, "d"), &Value::Int(1));
    assert_eq!(back.as_compound().unwrap().len(), 1);
}

/// The same rule on the little-endian path, stated as the negative: the result
/// must be `1`, i.e. *not* the last-wins value a plain map insert would give.
#[test]
fn duplicate_compound_keys_keep_the_first_value_not_the_last() {
    let bytes = hex(concat!(
        "0a0000", "03010061", "01000000", "03010061", "02000000", "00"
    ));
    let v: Value = from_le_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(
        v,
        compound([("a", Value::Int(1))]),
        "first duplicate wins; a plain map insert would have yielded 2"
    );
}

/// NBT files carry meaningful key order, so `Value::Compound` is backed by an
/// order-preserving `IndexMap` and a multi-key compound re-encodes
/// byte-identically.
///
/// Requires the default `preserve_order` feature; without it `Compound` is a
/// sorted `BTreeMap` and key order is normalised rather than preserved.
#[cfg(feature = "preserve_order")]
#[test]
fn key_order_is_preserved() {
    // Keys deliberately out of both insertion- and sort-order.
    // root; Int "z"=1; Int "a"=2; Int "m"=3; end
    let bytes = hex(concat!(
        "0a0000", "030100", "7a", "01000000", "030100", "61", "02000000", "030100", "6d",
        "03000000", "00"
    ));
    let v: Value = from_le_bytes(&mut bytes.as_slice()).unwrap();
    let Value::Compound(m) = &v else { panic!() };
    let keys: Vec<String> = m.keys().map(std::string::ToString::to_string).collect();
    assert_eq!(keys, vec!["z", "a", "m"], "on-disk key order must be kept");
    assert_eq!(
        to_le_bytes(&v).unwrap(),
        bytes,
        "must re-encode byte-identically"
    );
}
