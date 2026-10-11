//! Malformed, truncated and out-of-range binary input.
//!
//! The contract for every case here is the same: decoding returns `Err`. It must
//! never panic, abort, or silently resynchronise on garbage — NBT is routinely
//! parsed straight off a network socket, so a decoder that panics on bad bytes
//! is a remote denial of service.

#![cfg(feature = "nbt")]

use bstr::BString;
use nbtx::{Compound, Value, ValueList, from_be_bytes, from_le_bytes, to_be_bytes};

fn hex(s: &str) -> Vec<u8> {
    let packed: String = s.split_whitespace().collect();
    assert!(packed.len().is_multiple_of(2), "odd-length hex: {s}");
    (0..packed.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&packed[i..i + 2], 16).unwrap())
        .collect()
}

fn compound(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Compound(
        entries
            .into_iter()
            .map(|(k, v)| (BString::from(k), v))
            .collect::<Compound>(),
    )
}

/// Hand-assembles big-endian documents, so tests can craft inputs the encoder
/// would never produce.
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

/// Tag 13 does not exist; the highest defined tag is LongArray (12).
#[test]
fn unknown_tag_id_is_rejected_little_endian() {
    let bytes = hex(concat!("0a0000", "0d", "010061", "00"));
    assert!(from_le_bytes::<Value>(&mut bytes.as_slice()).is_err());
}

/// The full out-of-range space, including the values a corrupted length prefix
/// is most likely to land on.
#[test]
fn every_out_of_range_tag_id_is_rejected() {
    for bad in [13u8, 14, 200, 255] {
        let mut buf = be::root_compound();
        be::entry_header(&mut buf, bad, b"x");
        be::end(&mut buf);
        let res: Result<Value, _> = from_be_bytes(&mut buf.as_slice());
        assert!(res.is_err(), "tag id {bad} must be rejected");
    }
}

/// A list's element-type byte is subject to the same tag range as any other tag
/// byte.
#[test]
fn invalid_list_element_type_is_rejected() {
    let bytes = hex(concat!("0a0000", "09", "010061", "0d", "00000000", "00"));
    assert!(from_le_bytes::<Value>(&mut bytes.as_slice()).is_err());
}

/// TAG_End is the compound terminator, so it is a legal element type only for a
/// list that has no elements. A non-empty one has no readable payload.
#[test]
fn empty_list_of_tag_end_is_accepted_but_a_non_empty_one_is_not() {
    let empty = hex(concat!("0a0000", "09", "010061", "00", "00000000", "00"));
    let v: Value = from_le_bytes(&mut empty.as_slice()).unwrap();
    assert_eq!(
        v,
        compound([("a", Value::List(ValueList::End))]),
        "an empty TAG_End list is legal"
    );

    let nonempty = hex(concat!("0a0000", "09", "010061", "00", "01000000", "00"));
    assert!(from_le_bytes::<Value>(&mut nonempty.as_slice()).is_err());
}

/// The same rule with a length of 3 rather than 1.
#[test]
fn non_empty_list_of_tag_end_is_rejected() {
    let mut buf = be::root_compound();
    be::entry_header(&mut buf, 9, b"l");
    buf.push(0); // element type TAG_End
    buf.extend_from_slice(&3i32.to_be_bytes()); // but 3 elements
    be::end(&mut buf);
    let res: Result<Value, _> = from_be_bytes(&mut buf.as_slice());
    assert!(res.is_err(), "non-empty list of TAG_End must be rejected");
}

/// ...and with a length of exactly 1, the smallest non-empty case.
#[test]
fn list_of_tag_end_with_one_element_is_rejected() {
    let mut buf = be::root_compound();
    be::entry_header(&mut buf, 9, b"list");
    buf.push(0); // element type TAG_End
    buf.extend_from_slice(&1i32.to_be_bytes()); // length 1 (non-empty!)
    be::end(&mut buf);
    let res: Result<Value, _> = from_be_bytes(&mut buf.as_slice());
    assert!(res.is_err(), "non-empty list of TAG_End must be rejected");
}

/// A single `0x00` byte is a TAG_End with no payload, which is not a document.
#[test]
fn lone_tag_end_byte_is_rejected() {
    assert!(from_le_bytes::<Value>(&mut [0x00u8].as_slice()).is_err());
}

/// A TAG_End root followed by a name prefix is equally invalid.
#[test]
fn tag_end_at_the_root_is_rejected() {
    let res: Result<Value, _> = from_be_bytes(&mut [0u8, 0, 0].as_slice());
    assert!(res.is_err(), "TAG_End root must be rejected");
}

/// A payload that stops mid-value.
#[test]
fn truncated_payload_is_rejected() {
    let bytes = hex(concat!("0a0000", "03", "010061", "0100"));
    assert!(from_le_bytes::<Value>(&mut bytes.as_slice()).is_err());
}

/// Array and list lengths are read as a signed `i32`, so a negative value is
/// representable on the wire. It cannot be turned into an allocation, so every
/// length-prefixed tag must reject it.
#[test]
fn negative_array_and_list_lengths_are_rejected() {
    for tag in ["07", "0b", "0c"] {
        let bytes = hex(&format!("0a0000{tag}010061ffffffff00"));
        assert!(
            from_le_bytes::<Value>(&mut bytes.as_slice()).is_err(),
            "tag {tag}: negative length must error"
        );
    }
    let list = hex(concat!("0a0000", "09", "010061", "02", "ffffffff", "00"));
    assert!(from_le_bytes::<Value>(&mut list.as_slice()).is_err());
}

/// A small negative ByteArray length, big-endian.
#[test]
fn negative_byte_array_length_is_rejected() {
    let mut buf = be::root_compound();
    be::entry_header(&mut buf, 7, b"a");
    buf.extend_from_slice(&(-5i32).to_be_bytes());
    be::end(&mut buf);
    let res: Result<Value, _> = from_be_bytes(&mut buf.as_slice());
    assert!(res.is_err(), "negative ByteArray length must be rejected");
}

/// The same, asserted before any allocation is attempted (a negative length cast
/// to `usize` would be an enormous reservation).
#[test]
fn negative_array_length_is_rejected_before_allocating() {
    let mut buf = be::root_compound();
    be::entry_header(&mut buf, 7, b"a"); // TAG_ByteArray
    buf.extend_from_slice(&(-5i32).to_be_bytes()); // negative length
    be::end(&mut buf);
    let res: Result<Value, _> = from_be_bytes(&mut buf.as_slice());
    assert!(res.is_err(), "negative array length must be rejected");
}

/// Cutting a valid document at *every* offset must error at every one of them —
/// this is the cheap fuzz case that catches an unchecked read.
#[test]
fn every_truncation_point_errors() {
    let full = to_be_bytes(&compound([
        ("s", Value::String(BString::from("hello"))),
        ("ia", Value::IntArray(vec![1, 2, 3])),
        ("l", Value::List(ValueList::Long(vec![7]))),
    ]))
    .unwrap();
    for cut in 1..full.len() {
        let res: Result<Value, _> = from_be_bytes(&mut &full[..cut]);
        assert!(res.is_err(), "truncation at {cut} must error");
    }
}

/// The same sweep including the zero-length cut, plus an out-of-range root tag.
/// Only requirement: no panic.
#[test]
fn truncated_input_never_panics() {
    let value = compound([
        ("s", Value::String("hello".into())),
        ("a", Value::IntArray(vec![1, 2, 3, 4])),
    ]);
    let bytes = to_be_bytes(&value).unwrap();
    for cut in 0..bytes.len() {
        let _res: Result<Value, _> = from_be_bytes(&mut &bytes[..cut]);
    }
    let res: Result<Value, _> = from_be_bytes(&mut [99u8, 0, 0].as_slice());
    assert!(res.is_err());
}
