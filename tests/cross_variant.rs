//! Consistency *between* the three binary variants (and the textual one).
//!
//! Every other file checks each variant against itself: encode with `to_be_*`,
//! decode with `from_be_*`, compare. That catches a broken variant but not a
//! *divergent* one — three encoders that each round-trip perfectly can still
//! disagree about what document they describe, and the symptom only shows up
//! when a Bedrock world (little-endian) is read by a tool that talks the network
//! variant.
//!
//! The tests here therefore always compare **across** representations: the
//! decoded [`nbtx::Value`], not the bytes, is the thing asserted equal. Where a
//! document is hand-assembled it is written out three times, once per variant,
//! so a shared bug in the encoders cannot make the comparison vacuous.

#![cfg(feature = "nbt")]

use bstr::BString;
use nbtx::{
    Compound, Named, Value, ValueList, from_be_bytes, from_le_bytes, from_varint_bytes,
    to_be_bytes, to_le_bytes, to_varint_bytes,
};

const BIG_TEST_NBT: &[u8] = include_bytes!("fixtures/bigtest.nbt");
const HELLO_WORLD_NBT: &[u8] = include_bytes!("fixtures/hello_world.nbt");
const PLAYER_NAN_VALUE_NBT: &[u8] = include_bytes!("fixtures/player_nan_value.nbt");
const SERVERS_DAT: &[u8] = include_bytes!("../examples/servers.dat");

fn hex(s: &str) -> Vec<u8> {
    let packed: String = s.split_whitespace().collect();
    assert!(packed.len().is_multiple_of(2), "odd-length hex: {s}");
    (0..packed.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&packed[i..i + 2], 16).unwrap())
        .collect()
}

fn map(entries: &[(&str, Value)]) -> Compound {
    entries
        .iter()
        .map(|(k, v)| (BString::from(*k), v.clone()))
        .collect()
}

fn comp(entries: &[(&str, Value)]) -> Value {
    Value::Compound(map(entries))
}

/// A representative spread: every tag, both empty and populated containers,
/// non-UTF-8 payloads, and the numeric extremes that each variant encodes
/// differently.
fn representative_documents() -> Vec<(&'static str, Value)> {
    vec![
        ("scalar root", Value::Int(i32::MIN)),
        ("string root", Value::String(BString::from("hello"))),
        ("empty compound", comp(&[])),
        (
            "every tag",
            comp(&[
                ("byte", Value::Byte(-128)),
                ("short", Value::Short(-32768)),
                ("int", Value::Int(i32::MAX)),
                ("long", Value::Long(i64::MIN)),
                ("float", Value::Float(-0.0)),
                ("double", Value::Double(f64::MIN_POSITIVE)),
                ("bytes", Value::ByteArray(vec![0, 0x7f, 0x80, 0xff])),
                (
                    "string",
                    Value::String(BString::from(vec![0xffu8, 0x00, b'a'])),
                ),
                ("list", Value::List(ValueList::Short(vec![1, -1]))),
                ("ints", Value::IntArray(vec![i32::MIN, 0, i32::MAX])),
                ("longs", Value::LongArray(vec![i64::MIN, 0, i64::MAX])),
            ]),
        ),
        (
            "empty containers",
            comp(&[
                ("ba", Value::ByteArray(vec![])),
                ("ia", Value::IntArray(vec![])),
                ("la", Value::LongArray(vec![])),
                ("li", Value::List(ValueList::End)),
                ("co", comp(&[])),
            ]),
        ),
        (
            "nested",
            comp(&[(
                "a",
                Value::List(ValueList::Compound(vec![map(&[(
                    "b",
                    Value::List(ValueList::Long(vec![7])),
                )])])),
            )]),
        ),
    ]
}

/// One logical document, encoded three ways, must decode to one identical
/// `Value` — the property that makes a Bedrock disk file and the same data on
/// the wire interchangeable.
#[test]
fn every_variant_decodes_a_document_to_the_same_value() {
    for (label, doc) in representative_documents() {
        let be: Value = from_be_bytes(&mut to_be_bytes(&doc).unwrap().as_slice()).unwrap();
        let le: Value = from_le_bytes(&mut to_le_bytes(&doc).unwrap().as_slice()).unwrap();
        let var: Value = from_varint_bytes(&mut to_varint_bytes(&doc).unwrap().as_slice()).unwrap();
        assert_eq!(be, doc, "{label}: big-endian");
        assert_eq!(le, doc, "{label}: little-endian");
        assert_eq!(var, doc, "{label}: varint");
        assert_eq!(be, le, "{label}: BE and LE disagree");
        assert_eq!(le, var, "{label}: LE and varint disagree");
    }
}

/// The variants must be genuinely *different* byte strings for anything wider
/// than a byte — otherwise the previous test would pass trivially with three
/// copies of one encoder.
#[test]
fn the_variants_produce_different_bytes_for_the_same_document() {
    let doc = comp(&[("n", Value::Int(300)), ("s", Value::Short(300))]);
    let be = to_be_bytes(&doc).unwrap();
    let le = to_le_bytes(&doc).unwrap();
    let var = to_varint_bytes(&doc).unwrap();
    assert_ne!(be, le, "BE and LE must differ in byte order");
    assert_ne!(
        le, var,
        "LE and varint must differ in their length prefixes"
    );
    assert_ne!(be, var);
    // ...and the varint form is the shortest for small numbers, which is its
    // whole reason for existing on the network.
    assert!(
        var.len() < le.len(),
        "varint {} vs le {}",
        var.len(),
        le.len()
    );
}

/// Hand-assembled bytes, one per variant, describing the same document. Written
/// out by hand rather than produced by nbtx, so a shared encoder bug cannot make
/// the three agree wrongly.
#[test]
fn hand_written_bytes_for_each_variant_decode_alike() {
    // { "a": Int(300), "b": Short(-2), "c": "hi" }
    let expected = comp(&[
        ("a", Value::Int(300)),
        ("b", Value::Short(-2)),
        ("c", Value::String(BString::from("hi"))),
    ]);

    let be = hex(concat!(
        "0a 0000",              // root compound, empty name
        "03 0001 61 0000012c",  // Int "a" = 300
        "02 0001 62 fffe",      // Short "b" = -2
        "08 0001 63 0002 6869", // String "c" = "hi"
        "00"
    ));
    let le = hex(concat!(
        "0a 0000",
        "03 0100 61 2c010000",
        "02 0100 62 feff",
        "08 0100 63 0200 6869",
        "00"
    ));
    let var = hex(concat!(
        "0a 00",
        "03 01 61 d804", // zigzag(300) = 600 -> d8 04
        "02 01 62 feff", // Short stays fixed-width little-endian
        "08 01 63 02 6869",
        "00"
    ));

    assert_eq!(
        from_be_bytes::<Value>(&mut be.as_slice()).unwrap(),
        expected
    );
    assert_eq!(
        from_le_bytes::<Value>(&mut le.as_slice()).unwrap(),
        expected
    );
    assert_eq!(
        from_varint_bytes::<Value>(&mut var.as_slice()).unwrap(),
        expected
    );

    // And nbtx reproduces each of those byte strings exactly.
    assert_eq!(to_be_bytes(&expected).unwrap(), be);
    assert_eq!(to_le_bytes(&expected).unwrap(), le);
    assert_eq!(to_varint_bytes(&expected).unwrap(), var);
}

/// Transcoding a document round the three variants and back must land on the
/// original bytes. This is the operation a world converter performs, and it is
/// the one that would expose a variant-specific rounding or sign bug.
#[test]
fn a_document_survives_a_full_transcoding_cycle() {
    for (label, doc) in representative_documents() {
        let be = to_be_bytes(&doc).unwrap();

        let v1: Value = from_be_bytes(&mut be.as_slice()).unwrap();
        let le = to_le_bytes(&v1).unwrap();
        let v2: Value = from_le_bytes(&mut le.as_slice()).unwrap();
        let var = to_varint_bytes(&v2).unwrap();
        let v3: Value = from_varint_bytes(&mut var.as_slice()).unwrap();

        assert_eq!(
            to_be_bytes(&v3).unwrap(),
            be,
            "{label}: BE -> LE -> varint -> BE"
        );
    }
}

/// The same cycle for the real fixtures, which carry shapes the synthetic
/// documents do not (a 1000-byte `ByteArray`, a NaN `Double`, nested lists of
/// compounds).
#[test]
fn fixtures_survive_a_full_transcoding_cycle() {
    for (label, fixture) in [
        ("bigtest", BIG_TEST_NBT),
        ("hello_world", HELLO_WORLD_NBT),
        ("player_nan_value", PLAYER_NAN_VALUE_NBT),
        ("servers.dat", SERVERS_DAT),
    ] {
        let v: Value = from_be_bytes(&mut fixture.to_vec().as_slice()).unwrap();
        let be = to_be_bytes(&v).unwrap();

        let le = to_le_bytes(&v).unwrap();
        let via_le: Value = from_le_bytes(&mut le.as_slice()).unwrap();
        let var = to_varint_bytes(&via_le).unwrap();
        let via_var: Value = from_varint_bytes(&mut var.as_slice()).unwrap();

        assert_eq!(
            to_be_bytes(&via_var).unwrap(),
            be,
            "{label}: transcoding through LE and varint changed the document"
        );
    }
}

/// `player_nan_value.nbt` contains a NaN, which `PartialEq` cannot compare, so
/// the fixture cycle above relies on byte equality. This states the NaN claim
/// directly: its exact bit pattern is the same after a trip through every
/// variant.
#[test]
fn a_nan_in_a_fixture_keeps_its_bits_across_variants() {
    fn nan_bits(v: &Value) -> Vec<u64> {
        match v {
            Value::Double(d) if d.is_nan() => vec![d.to_bits()],
            Value::Float(f) if f.is_nan() => vec![u64::from(f.to_bits())],
            // `ValueList` stores its elements unboxed, so the two float
            // element types are read straight out of their vectors rather than
            // through a per-element `Value`.
            Value::List(ValueList::Float(items)) => items
                .iter()
                .filter(|f| f.is_nan())
                .map(|f| u64::from(f.to_bits()))
                .collect(),
            Value::List(ValueList::Double(items)) => items
                .iter()
                .filter(|d| d.is_nan())
                .map(|d| d.to_bits())
                .collect(),
            Value::List(ValueList::List(items)) => items
                .iter()
                .flat_map(|l| nan_bits(&Value::List(l.clone())))
                .collect(),
            Value::List(ValueList::Compound(items)) => items
                .iter()
                .flat_map(|m| nan_bits(&Value::Compound(m.clone())))
                .collect(),
            // Every other element type is an integer, a string or an array:
            // nothing that can be a NaN.
            Value::List(_) => Vec::new(),
            Value::Compound(entries) => entries.values().flat_map(nan_bits).collect(),
            _ => Vec::new(),
        }
    }

    let original: Value = from_be_bytes(&mut PLAYER_NAN_VALUE_NBT.to_vec().as_slice()).unwrap();
    let expected = nan_bits(&original);
    assert!(
        !expected.is_empty(),
        "the fixture must actually contain a NaN"
    );

    for (label, bytes) in [
        ("LE", to_le_bytes(&original).unwrap()),
        ("varint", to_varint_bytes(&original).unwrap()),
    ] {
        let back: Value = if label == "LE" {
            from_le_bytes(&mut bytes.as_slice()).unwrap()
        } else {
            from_varint_bytes(&mut bytes.as_slice()).unwrap()
        };
        assert_eq!(nan_bits(&back), expected, "{label}: NaN bits changed");
    }
}

/// A named root must survive transcoding too: the root name is written with the
/// same string codec as any other NBT string, so it changes width between the
/// fixed-width and varint variants.
#[test]
fn a_named_root_transcodes_across_variants() {
    let doc = Named::new(
        "a rather long root name",
        comp(&[("x", Value::Int(1_000_000))]),
    );
    let be = to_be_bytes(&doc).unwrap();
    let le = to_le_bytes(&doc).unwrap();
    let var = to_varint_bytes(&doc).unwrap();

    let a: Named<Value> = from_be_bytes(&mut be.as_slice()).unwrap();
    let b: Named<Value> = from_le_bytes(&mut le.as_slice()).unwrap();
    let c: Named<Value> = from_varint_bytes(&mut var.as_slice()).unwrap();
    assert_eq!(a, b);
    assert_eq!(b, c);
    assert_eq!(a.name, "a rather long root name");
    // The root name goes through the ordinary NBT string codec, so its length
    // prefix is a `u16` in the fixed-width variants and a one-byte varuint here:
    // the name itself therefore starts one byte earlier in the varint stream.
    assert_eq!(&be[3..26], b"a rather long root name");
    assert_eq!(&le[3..26], b"a rather long root name");
    assert_eq!(&var[2..25], b"a rather long root name");
}

/// The dynamic `Value` path and the derived-struct path must agree on the same
/// bytes in every variant — they share no code below the entry points.
#[test]
fn the_value_and_struct_paths_agree_in_every_variant() {
    #[derive(facet::Facet, Debug, PartialEq)]
    struct S {
        a: i32,
        b: String,
        c: Vec<i64>,
    }
    let s = S {
        a: -7,
        b: "text".to_owned(),
        c: vec![1, -2],
    };
    let as_value = comp(&[
        ("a", Value::Int(-7)),
        ("b", Value::String(BString::from("text"))),
        ("c", Value::LongArray(vec![1, -2])),
    ]);

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            assert_eq!(
                $to(&s).unwrap(),
                $to(&as_value).unwrap(),
                concat!(
                    stringify!($to),
                    ": struct and Value must encode identically"
                )
            );
            let bytes = $to(&s).unwrap();
            assert_eq!($from::<Value>(&mut bytes.as_slice()).unwrap(), as_value);
            assert_eq!($from::<S>(&mut bytes.as_slice()).unwrap(), s);
        }};
    }
    check!(to_be_bytes, from_be_bytes);
    check!(to_le_bytes, from_le_bytes);
    check!(to_varint_bytes, from_varint_bytes);
}

/// The textual codec is a fourth representation of the same model, so a document
/// that has been through SNBT must equal one that has been through each binary
/// variant.
#[cfg(feature = "snbt")]
#[test]
fn the_textual_codec_agrees_with_all_three_binary_variants() {
    for (label, doc) in representative_documents() {
        // SNBT is text and renders raw bytes lossily, so compare only the
        // documents whose strings are valid UTF-8.
        let snbt = match nbtx::to_string(&doc) {
            Ok(s) => s,
            Err(e) => panic!("{label}: {e}"),
        };
        let via_text: Value = nbtx::from_string(&snbt).unwrap();
        let via_be: Value = from_be_bytes(&mut to_be_bytes(&doc).unwrap().as_slice()).unwrap();

        if label == "every tag" {
            // This one carries a deliberately non-UTF-8 string; SNBT cannot
            // represent it, so only the binary variants are compared for it.
            assert_eq!(via_be, doc);
            continue;
        }
        assert_eq!(via_text, via_be, "{label}: SNBT and big-endian disagree");
    }
}

/// `bigtest.nbt` through SNBT and through each binary variant must land on one
/// `Value` — the widest real document available, and the one whose float and
/// `ByteArray` payloads most stress the textual form.
#[cfg(feature = "snbt")]
#[test]
fn bigtest_agrees_across_all_four_representations() {
    let original: Value = from_be_bytes(&mut BIG_TEST_NBT.to_vec().as_slice()).unwrap();
    let via_le: Value = from_le_bytes(&mut to_le_bytes(&original).unwrap().as_slice()).unwrap();
    let via_var: Value =
        from_varint_bytes(&mut to_varint_bytes(&original).unwrap().as_slice()).unwrap();
    let via_snbt: Value = nbtx::from_string(nbtx::to_string(&original).unwrap()).unwrap();

    assert_eq!(via_le, original);
    assert_eq!(via_var, original);
    assert_eq!(via_snbt, original);
}
