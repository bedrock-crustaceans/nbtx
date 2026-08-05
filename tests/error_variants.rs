//! One test per [`nbtx::Error`] variant, plus the public accessors each one
//! carries.
//!
//! Elsewhere a failing decode is usually asserted with `is_err()`, which cannot
//! tell a malformed-framing error from a plain end-of-file — a distinction that
//! matters to callers, because EOF is recoverable in a streaming reader and
//! corrupt framing is not. These tests pin *which* variant each input produces
//! and read back the payload the variant promises, so an error that silently
//! degrades to `Error::Other` is caught.
//!
//! Every variant of `Error` is reachable from some input, and each has a test
//! below. `Error::ExpectedNumber` and `Error::IntegerTooLarge` used to be the
//! exceptions — declared and exported but constructed nowhere, because the SNBT
//! parser reports `ParseIntError`/`ParseFloatError` (which carry std's own
//! error, including the overflow case `IntegerTooLarge` described) and the
//! binary codec reads fixed-width integers that cannot overflow their tag. They
//! were removed from the enum in 4.0 rather than given a synthetic construction
//! site.

use nbtx::Error;

#[cfg(any(feature = "nbt", feature = "snbt"))]
use nbtx::Value;

#[cfg(feature = "nbt")]
use bstr::BString;
#[cfg(feature = "nbt")]
use nbtx::{Compound, from_be_bytes, from_le_bytes, from_varint_bytes, to_be_bytes, to_le_bytes};

#[cfg(feature = "nbt")]
fn hex(s: &str) -> Vec<u8> {
    let packed: String = s.split_whitespace().collect();
    assert!(packed.len().is_multiple_of(2), "odd-length hex: {s}");
    (0..packed.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&packed[i..i + 2], 16).unwrap())
        .collect()
}

#[cfg(feature = "nbt")]
fn comp(entries: &[(&str, Value)]) -> Value {
    Value::Compound(
        entries
            .iter()
            .map(|(k, v)| (BString::from(*k), v.clone()))
            .collect::<Compound>(),
    )
}

// --- nbt-only variants ----------------------------------------------------

/// `TypeOutOfRange` is the specific variant for a tag byte above 12 — not a
/// generic parse failure — and it reports the byte it saw so a caller can log
/// the corruption.
#[cfg(feature = "nbt")]
#[test]
fn type_out_of_range_carries_the_offending_tag_byte() {
    let bytes = hex(concat!("0a0000", "0d", "000178", "00"));
    let err = from_be_bytes::<Value>(&mut bytes.as_slice()).expect_err("tag 13 must be rejected");
    match err {
        Error::TypeOutOfRange(e) => assert_eq!(e.found(), 0x0d),
        other => panic!("expected TypeOutOfRange, got {other:?}"),
    }
}

/// The root tag byte goes through the same check, so a garbage first byte is
/// reported as an out-of-range tag rather than as EOF.
#[cfg(feature = "nbt")]
#[test]
fn type_out_of_range_applies_to_the_root_tag_too() {
    let err = from_be_bytes::<Value>(&mut [0xffu8, 0, 0].as_slice()).expect_err("must reject");
    assert!(
        matches!(&err, Error::TypeOutOfRange(e) if e.found() == 0xff),
        "{err:?}"
    );
}

/// A `TAG_End` root is not a document. It has its own variant because it is a
/// *structurally* different failure from a bad tag: `0x00` is a perfectly legal
/// byte, just not at the start of a document.
#[cfg(feature = "nbt")]
#[test]
fn unexpected_end_for_a_tag_end_root() {
    let err = from_be_bytes::<Value>(&mut [0u8, 0, 0].as_slice()).expect_err("must reject");
    assert!(matches!(err, Error::UnexpectedEnd(_)), "{err:?}");
}

/// The other way to reach `UnexpectedEnd`: a list that declares `TAG_End` as its
/// element type but claims a non-zero length, so the reader is asked to read an
/// End payload that does not exist.
#[cfg(feature = "nbt")]
#[test]
fn unexpected_end_for_a_non_empty_list_of_tag_end() {
    let bytes = hex(concat!("0a0000", "09", "00016c", "00", "00000002", "00"));
    let err = from_be_bytes::<Value>(&mut bytes.as_slice()).expect_err("must reject");
    assert!(matches!(err, Error::UnexpectedEnd(_)), "{err:?}");
}

/// `UnknownField` names both the key and the struct, and its message points at
/// the attribute that opts out — the error is a schema-drift report, so it has
/// to be actionable.
#[cfg(feature = "nbt")]
#[test]
fn unknown_field_names_the_key_the_container_and_the_opt_out() {
    #[derive(facet::Facet, Debug)]
    struct Player {
        health: i32,
    }
    let doc = comp(&[("health", Value::Int(20)), ("mana", Value::Int(5))]);
    let bytes = to_be_bytes(&doc).unwrap();
    let err = from_be_bytes::<Player>(&mut bytes.as_slice()).expect_err("must reject `mana`");
    match &err {
        Error::UnknownField(e) => {
            assert_eq!(e.field(), "mana");
            assert_eq!(e.container(), "Player");
        }
        other => panic!("expected UnknownField, got {other:?}"),
    }
    let msg = err.to_string();
    assert!(msg.contains("mana") && msg.contains("Player"), "{msg}");
    assert!(
        msg.contains("allow_unknown_fields"),
        "the message must name the opt-out attribute: {msg}"
    );
}

/// `StringTooLong` reports both the rejected length and the limit, so a caller
/// can tell "slightly over" from "wildly corrupt".
#[cfg(feature = "nbt")]
#[test]
fn string_too_long_reports_the_length_and_the_limit() {
    let doc = comp(&[(
        "s",
        Value::String(BString::from(vec![b'a'; nbtx::MAX_STRING_LEN + 1])),
    )]);
    let err = to_be_bytes(&doc).expect_err("must reject");
    match err {
        Error::StringTooLong(e) => {
            assert_eq!(e.len(), nbtx::MAX_STRING_LEN + 1);
            assert_eq!(e.max(), nbtx::MAX_STRING_LEN);
            assert!(!e.is_empty());
        }
        other => panic!("expected StringTooLong, got {other:?}"),
    }
}

/// The same variant on the *read* side, which only the varint variant can reach
/// (the fixed-width ones are bounded by their `u16` prefix). Asserting the
/// variant, not just `is_err`, is what distinguishes the length check from the
/// EOF that would follow it.
#[cfg(feature = "nbt")]
#[test]
fn string_too_long_is_raised_on_the_varint_read_path() {
    // varuint32 32768 = 80 80 02, with no payload behind it.
    let bytes = hex(concat!("0a00", "0801", "61", "808002"));
    let err = from_varint_bytes::<Value>(&mut bytes.as_slice()).expect_err("must reject");
    match err {
        Error::StringTooLong(e) => {
            assert_eq!(e.len(), nbtx::MAX_STRING_LEN + 1);
            assert_eq!(e.max(), nbtx::MAX_STRING_LEN);
        }
        other => panic!("expected StringTooLong, got {other:?}"),
    }
}

/// `InvalidVarint` reports the byte cap that was exceeded, which differs between
/// the 32- and 64-bit paths (5 vs 10). Reading it back proves the error is
/// raised by the right reader.
#[cfg(feature = "nbt")]
#[test]
fn invalid_varint_reports_the_width_it_was_bounded_to() {
    let int = hex(concat!("0a00", "0301", "61", "ffffffffff7f", "00"));
    match from_varint_bytes::<Value>(&mut int.as_slice()).expect_err("must reject") {
        Error::InvalidVarint(e) => assert_eq!(e.max_bytes(), 5, "a varint32 is capped at 5 bytes"),
        other => panic!("expected InvalidVarint, got {other:?}"),
    }

    let long = hex(concat!(
        "0a00",
        "0401",
        "61",
        "ffffffffffffffffffff7f",
        "00"
    ));
    match from_varint_bytes::<Value>(&mut long.as_slice()).expect_err("must reject") {
        Error::InvalidVarint(e) => {
            assert_eq!(e.max_bytes(), 10, "a varint64 is capped at 10 bytes")
        }
        other => panic!("expected InvalidVarint, got {other:?}"),
    }
}

/// `MaxDepthExceeded` carries the limit it hit, so a caller can report it
/// without hard-coding 512.
#[cfg(feature = "nbt")]
#[test]
fn max_depth_exceeded_reports_the_limit_it_hit() {
    let mut bytes = hex("0a0000");
    for _ in 0..nbtx::MAX_DEPTH + 5 {
        bytes.extend(hex("0a010061"));
    }
    bytes.extend(std::iter::repeat_n(0x00u8, nbtx::MAX_DEPTH + 6));
    match from_le_bytes::<Value>(&mut bytes.as_slice()).expect_err("must reject") {
        Error::MaxDepthExceeded(e) => assert_eq!(e.max(), nbtx::MAX_DEPTH),
        other => panic!("expected MaxDepthExceeded, got {other:?}"),
    }
}

/// A truncated payload must surface as the dedicated EOF variant rather than as
/// `Error::Other` — a streaming caller retries on EOF and gives up on anything
/// else.
#[cfg(feature = "nbt")]
#[test]
fn unexpected_eof_for_a_stream_that_stops_mid_value() {
    let bytes = hex(concat!("0a0000", "03", "000161", "0000"));
    let err = from_be_bytes::<Value>(&mut bytes.as_slice()).expect_err("must reject");
    assert!(matches!(err, Error::UnexpectedEof(_)), "{err:?}");
}

/// An array whose declared length runs past the end of the buffer is EOF too,
/// not a length-limit error: the bounded reader never pre-allocates from the
/// wire length, so it simply runs out of bytes.
#[cfg(feature = "nbt")]
#[test]
fn unexpected_eof_for_an_array_longer_than_its_data() {
    let mut bytes = hex(concat!("0a0000", "07", "000161"));
    bytes.extend_from_slice(&0x0010_0000i32.to_be_bytes()); // claims 1 MiB
    bytes.extend_from_slice(&[1, 2, 3]); // ...supplies three bytes
    let err = from_be_bytes::<Value>(&mut bytes.as_slice()).expect_err("must reject");
    assert!(matches!(err, Error::UnexpectedEof(_)), "{err:?}");
}

/// `UnexpectedType` reports both the tag it wanted and the tag it found, which
/// is the only way a caller can tell a schema mismatch from data corruption.
#[cfg(feature = "nbt")]
#[test]
fn unexpected_type_reports_both_the_expected_and_the_found_tag() {
    #[derive(facet::Facet, Debug)]
    struct S {
        a: i32,
    }
    // The wire says Long; the field is an i32, i.e. an Int tag.
    let doc = comp(&[("a", Value::Long(1))]);
    let bytes = to_be_bytes(&doc).unwrap();
    match from_be_bytes::<S>(&mut bytes.as_slice()).expect_err("must reject") {
        Error::UnexpectedType(e) => {
            assert_eq!(e.expected(), nbtx::FieldType::Int);
            assert_eq!(e.found(), nbtx::FieldType::Long);
        }
        other => panic!("expected UnexpectedType, got {other:?}"),
    }
}

/// The same variant when the mismatch is at the *root*: a scalar document
/// decoded into a struct, and a compound decoded into a scalar.
#[cfg(feature = "nbt")]
#[test]
fn unexpected_type_at_the_root_names_the_container_tag() {
    #[derive(facet::Facet, Debug)]
    struct S {
        a: i32,
    }
    let scalar = to_be_bytes(&Value::Int(1)).unwrap();
    match from_be_bytes::<S>(&mut scalar.as_slice()).expect_err("must reject") {
        Error::UnexpectedType(e) => {
            assert_eq!(e.expected(), nbtx::FieldType::Compound);
            assert_eq!(e.found(), nbtx::FieldType::Int);
        }
        other => panic!("expected UnexpectedType, got {other:?}"),
    }

    let compound = to_be_bytes(&comp(&[("a", Value::Int(1))])).unwrap();
    match from_be_bytes::<i32>(&mut compound.as_slice()).expect_err("must reject") {
        Error::UnexpectedType(e) => {
            assert_eq!(e.expected(), nbtx::FieldType::Int);
            assert_eq!(e.found(), nbtx::FieldType::Compound);
        }
        other => panic!("expected UnexpectedType, got {other:?}"),
    }
}

/// `HeterogeneousList` names the tag it settled on and the first one that
/// differed, so the offending element can be found without a bisect.
#[cfg(feature = "nbt")]
#[test]
fn heterogeneous_list_names_the_expected_and_offending_tags() {
    let doc = Value::List(vec![Value::Byte(1), Value::Byte(2), Value::Int(3)]);
    match to_be_bytes(&doc).expect_err("must reject") {
        Error::HeterogeneousList { expected, found } => {
            assert_eq!(expected, nbtx::FieldType::Byte);
            assert_eq!(found, nbtx::FieldType::Int);
        }
        other => panic!("expected HeterogeneousList, got {other:?}"),
    }
}

/// `Unsupported` covers the shapes the codec deliberately refuses. Each carries
/// a static description, and the three cases below are the ones a user can
/// actually hit: an enum with data, a bare `None`, and a map keyed by anything
/// but a string.
#[cfg(feature = "nbt")]
#[test]
fn unsupported_describes_the_operation_it_refused() {
    #[derive(facet::Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum WithData {
        A(i32),
        B,
    }
    match to_be_bytes(&WithData::A(1)).expect_err("an enum with data must be refused") {
        Error::Unsupported(e) => assert!(e.operation().contains("enums with data"), "{e:?}"),
        other => panic!("expected Unsupported, got {other:?}"),
    }

    match to_be_bytes(&Option::<i32>::None).expect_err("a bare None must be refused") {
        Error::Unsupported(e) => assert!(e.operation().contains("None"), "{e:?}"),
        other => panic!("expected Unsupported, got {other:?}"),
    }

    let map: std::collections::BTreeMap<i32, i32> = [(1, 2)].into_iter().collect();
    match to_be_bytes(&map).expect_err("a non-string map key must be refused") {
        Error::Unsupported(e) => assert!(e.operation().contains("map keys"), "{e:?}"),
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

/// A Rust scalar with no NBT tag (`u32` here — NBT has no unsigned 32-bit type)
/// is refused rather than silently widened into a `Long`.
#[cfg(feature = "nbt")]
#[test]
fn unsupported_for_a_scalar_with_no_nbt_tag() {
    #[derive(facet::Facet, Debug)]
    struct S {
        a: u32,
    }
    match to_be_bytes(&S { a: 1 }).expect_err("u32 has no NBT tag") {
        Error::Unsupported(e) => assert!(e.operation().contains("scalar"), "{e:?}"),
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

/// `Error::Other` is the catch-all for failures raised below nbtx — a UTF-8
/// violation from `String::from_utf8`, or a facet reflection error. Both are
/// reachable from ordinary input, so both are pinned here.
#[cfg(feature = "nbt")]
#[test]
fn other_wraps_utf8_and_reflection_failures() {
    #[derive(facet::Facet, Debug)]
    struct S {
        a: String,
    }
    // A `String` field demands valid UTF-8; the wire carries raw bytes.
    let doc = comp(&[("a", Value::String(BString::from(vec![0xffu8, 0xfe])))]);
    let bytes = to_be_bytes(&doc).unwrap();
    match from_be_bytes::<S>(&mut bytes.as_slice()).expect_err("must reject") {
        Error::Other(msg) => assert!(msg.contains("utf-8"), "{msg}"),
        other => panic!("expected Other, got {other:?}"),
    }

    // A required field that the document does not supply is reported by facet's
    // `Partial::build`, which nbtx surfaces as `Other`.
    #[derive(facet::Facet, Debug)]
    struct T {
        a: i32,
        b: i32,
    }
    let partial = to_be_bytes(&comp(&[("a", Value::Int(1))])).unwrap();
    match from_be_bytes::<T>(&mut partial.as_slice()).expect_err("must reject") {
        Error::Other(msg) => assert!(msg.contains('b'), "the message must name the field: {msg}"),
        other => panic!("expected Other, got {other:?}"),
    }
}

/// A `bstr::BString` field takes raw bytes, so the same non-UTF-8 document that
/// fails into a `String` must succeed into a `BString`. This is the escape hatch
/// the `Other`/UTF-8 error above points at.
#[cfg(feature = "nbt")]
#[test]
fn the_utf8_failure_has_a_bstring_escape_hatch() {
    #[derive(facet::Facet, Debug)]
    struct S {
        a: BString,
    }
    let raw = BString::from(vec![0xffu8, 0xfe]);
    let doc = comp(&[("a", Value::String(raw.clone()))]);
    let bytes = to_le_bytes(&doc).unwrap();
    let s: S = from_le_bytes(&mut bytes.as_slice()).expect("a BString field takes raw bytes");
    assert_eq!(s.a, raw);
}

// --- snbt-only variants ---------------------------------------------------

/// `UnexpectedSymbol` reports the character it found and, where the grammar
/// demanded a specific one, the character it wanted.
#[cfg(feature = "snbt")]
#[test]
fn unexpected_symbol_reports_what_it_found_and_wanted() {
    match nbtx::from_string::<Value>("{a 1}").expect_err("a missing colon must be rejected") {
        Error::UnexpectedSymbol(e) => {
            assert_eq!(e.found(), '1');
            assert_eq!(e.expected(), Some(':'));
        }
        other => panic!("expected UnexpectedSymbol, got {other:?}"),
    }

    // Where any of several characters would have been valid, `expected` is None.
    match nbtx::from_string::<Value>("{a:1 2}").expect_err("a missing separator must be rejected") {
        Error::UnexpectedSymbol(e) => assert_eq!(e.expected(), None),
        other => panic!("expected UnexpectedSymbol, got {other:?}"),
    }
}

/// An integer literal outside its suffix's range is a `ParseIntError`, carrying
/// the `std` error so the caller can distinguish overflow from a bad digit.
#[cfg(feature = "snbt")]
#[test]
fn parse_int_error_wraps_the_std_error() {
    use std::num::IntErrorKind;

    match nbtx::from_string::<Value>("{a:128b}").expect_err("128 does not fit an i8") {
        Error::ParseIntError(e) => assert_eq!(e.error().kind(), &IntErrorKind::PosOverflow),
        other => panic!("expected ParseIntError, got {other:?}"),
    }
    match nbtx::from_string::<Value>("{a:1.5b}").expect_err("1.5 is not an integer") {
        Error::ParseIntError(e) => assert_eq!(e.error().kind(), &IntErrorKind::InvalidDigit),
        other => panic!("expected ParseIntError, got {other:?}"),
    }
}

/// A malformed float literal is a `ParseFloatError`, likewise wrapping the `std`
/// error rather than flattening it into a string.
#[cfg(feature = "snbt")]
#[test]
fn parse_float_error_wraps_the_std_error() {
    match nbtx::from_string::<Value>("{a:1.2.3f}").expect_err("1.2.3 is not a float") {
        Error::ParseFloatError(e) => assert!(!e.error().to_string().is_empty()),
        other => panic!("expected ParseFloatError, got {other:?}"),
    }
}

/// The SNBT parser's end-of-input is the same `UnexpectedEof` variant the binary
/// reader uses, so a caller feeding text in chunks can treat the two alike.
#[cfg(feature = "snbt")]
#[test]
fn unexpected_eof_for_truncated_snbt() {
    for input in ["{a:", "{", "\"unterminated", "[1,"] {
        let err = nbtx::from_string::<Value>(input).expect_err("must reject {input}");
        assert!(matches!(err, Error::UnexpectedEof(_)), "{input:?}: {err:?}");
    }
}

/// `MaxDepthExceeded` is shared by both codecs, so the SNBT parser must raise
/// the identical variant with the identical limit.
#[cfg(feature = "snbt")]
#[test]
fn max_depth_exceeded_is_the_same_variant_in_snbt() {
    let input = format!("{}1{}", "{a:".repeat(1000), "}".repeat(1000));
    match nbtx::from_string::<Value>(&input).expect_err("must reject") {
        Error::MaxDepthExceeded(e) => assert_eq!(e.max(), nbtx::MAX_DEPTH),
        other => panic!("expected MaxDepthExceeded, got {other:?}"),
    }
}

// --- error-type plumbing --------------------------------------------------

/// Every error must render a non-empty message and be usable as a
/// `std::error::Error`, which is what `?` into `Box<dyn Error>` and most logging
/// frameworks require.
#[test]
fn errors_display_and_implement_the_std_error_trait() {
    let errors: Vec<Error> = vec![
        Error::Other(String::from("boom")),
        Error::HeterogeneousList {
            expected: nbtx::FieldType::Byte,
            found: nbtx::FieldType::Int,
        },
    ];
    for e in errors {
        assert!(!e.to_string().is_empty());
        assert!(!format!("{e:?}").is_empty());
        let dynamic: &dyn std::error::Error = &e;
        assert!(!dynamic.to_string().is_empty());
        // `Error` is `Clone`, so a caller can keep one after handling it.
        let _cloned = e.clone();
    }
}

/// The `HeterogeneousList` message must name both tags in words, since it is the
/// one variant whose payload is not reachable through an accessor method.
#[test]
fn heterogeneous_list_message_names_both_tags() {
    let e = Error::HeterogeneousList {
        expected: nbtx::FieldType::ByteArray,
        found: nbtx::FieldType::LongArray,
    };
    let msg = e.to_string();
    assert!(msg.contains("byte array"), "{msg}");
    assert!(msg.contains("long array"), "{msg}");
}

/// With the `error-context` feature every context-carrying variant gains `at()`
/// and `index()` accessors. They are part of the public API under that feature,
/// so they must at least be callable and return something sane — nothing else in
/// the suite compiles them.
#[cfg(all(feature = "error-context", feature = "nbt"))]
#[test]
fn error_context_accessors_are_available_under_the_feature() {
    let bytes = hex(concat!("0a0000", "03", "000161", "0000"));
    match from_be_bytes::<Value>(&mut bytes.as_slice()).expect_err("must reject") {
        Error::UnexpectedEof(e) => {
            assert!(!e.at().is_empty());
            assert!(e.index().is_none() || e.index().is_some());
        }
        other => panic!("expected UnexpectedEof, got {other:?}"),
    }

    let bad = hex(concat!("0a0000", "0d", "000178", "00"));
    match from_be_bytes::<Value>(&mut bad.as_slice()).expect_err("must reject") {
        Error::TypeOutOfRange(e) => {
            assert_eq!(e.found(), 0x0d);
            assert!(!e.at().is_empty());
            let _ = e.index();
        }
        other => panic!("expected TypeOutOfRange, got {other:?}"),
    }
}

/// `nbtx::Result<T>` is the crate's alias, and it must be usable as an ordinary
/// `Result` in a `?` chain — the reason it is exported at all.
#[cfg(feature = "nbt")]
#[test]
fn the_result_alias_composes_with_the_question_mark_operator() {
    fn roundtrip(v: &Value) -> nbtx::Result<Value> {
        let bytes = to_be_bytes(v)?;
        from_be_bytes(&mut bytes.as_slice())
    }
    assert_eq!(roundtrip(&Value::Int(1)).unwrap(), Value::Int(1));
    assert!(roundtrip(&Value::List(vec![Value::Byte(1), Value::Int(1)])).is_err());
}
