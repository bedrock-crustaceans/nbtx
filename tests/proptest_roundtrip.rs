//! Property-based round-trip tests over generated [`nbtx::Value`] trees.
//!
//! The hand-written tests elsewhere pin *specific* documents; these generate
//! hundreds of them per run, covering combinations nobody would think to write
//! down — a `LongArray` nested three lists deep, a compound whose key is a lone
//! `+`, a `Float` whose bits happen to spell a signalling NaN. For a binary
//! codec that is the highest-yield technique available: every generated case
//! exercises the same encode/decode paths the fixtures do, but with inputs the
//! author did not choose.
//!
//! Floats are generated *from their bit patterns*, so NaNs and negative zero
//! occur naturally. That makes `==` useless as an oracle (`NaN != NaN`, and
//! `-0.0 == 0.0` despite different bytes), so the binary properties compare with
//! [`bit_eq`], which is exact.
//!
//! Lists are generated homogeneously on purpose: the wire format has one
//! element-type byte per list, so a heterogeneous `Value::List` has no encoding
//! at all (see `tag_semantics::list_heterogeneous_is_rejected_on_encode`).
//! Generating them would only ever re-discover that error.

#![cfg(feature = "nbt")]

use bstr::BString;
use nbtx::{
    Compound, Value, from_be_bytes, from_le_bytes, from_varint_bytes, to_be_bytes, to_le_bytes,
    to_varint_bytes,
};
use proptest::prelude::*;

/// Invokes a caller-defined `check!($to, $from)` macro once per endianness.
/// Passing the function *identifiers* (rather than binding them to a `let`)
/// keeps each call an independent generic instantiation — an array of function
/// pointers cannot express the higher-ranked lifetimes these signatures carry.
macro_rules! for_each_endian {
    ($check:ident) => {
        $check!(to_be_bytes, from_be_bytes);
        $check!(to_le_bytes, from_le_bytes);
        $check!(to_varint_bytes, from_varint_bytes);
    };
}

/// Strings of arbitrary Unicode, so multi-byte sequences and control characters
/// appear in both keys and values.
fn arb_utf8_string(max: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(any::<char>(), 0..max).prop_map(|cs| cs.into_iter().collect())
}

/// NBT strings are length-prefixed raw bytes, so a generator that only produced
/// UTF-8 would never reach the paths that matter for Bedrock data.
fn arb_raw_string(max: usize) -> impl Strategy<Value = BString> {
    proptest::collection::vec(any::<u8>(), 0..max).prop_map(BString::from)
}

fn arb_key(utf8: bool) -> BoxedStrategy<BString> {
    if utf8 {
        arb_utf8_string(6).prop_map(BString::from).boxed()
    } else {
        arb_raw_string(6).boxed()
    }
}

/// The ten non-recursive tags. Floats come from `u32`/`u64` bit patterns so the
/// generator reaches NaN payloads, subnormals and `-0.0` without special-casing.
fn arb_leaf(utf8: bool) -> BoxedStrategy<Value> {
    let string = if utf8 {
        arb_utf8_string(8)
            .prop_map(|s| Value::String(BString::from(s)))
            .boxed()
    } else {
        arb_raw_string(8).prop_map(Value::String).boxed()
    };
    prop_oneof![
        any::<i8>().prop_map(Value::Byte),
        any::<i16>().prop_map(Value::Short),
        any::<i32>().prop_map(Value::Int),
        any::<i64>().prop_map(Value::Long),
        any::<u32>().prop_map(|b| Value::Float(f32::from_bits(b))),
        any::<u64>().prop_map(|b| Value::Double(f64::from_bits(b))),
        proptest::collection::vec(any::<u8>(), 0..12).prop_map(Value::ByteArray),
        string,
        proptest::collection::vec(any::<i32>(), 0..6).prop_map(Value::IntArray),
        proptest::collection::vec(any::<i64>(), 0..6).prop_map(Value::LongArray),
    ]
    .boxed()
}

/// Drops every element that does not share the first one's tag, because a
/// heterogeneous list cannot be encoded at all.
fn homogenise(mut items: Vec<Value>) -> Vec<Value> {
    let Some(first) = items.first().map(Value::discriminant) else {
        return items;
    };
    items.retain(|v| v.discriminant() == first);
    items
}

/// Whole `Value` trees, at most five containers deep — well inside
/// [`nbtx::MAX_DEPTH`], which has its own dedicated tests.
fn arb_value(utf8: bool) -> BoxedStrategy<Value> {
    arb_leaf(utf8)
        .prop_recursive(4, 40, 4, move |inner| {
            prop_oneof![
                proptest::collection::vec(inner.clone(), 0..4)
                    .prop_map(|items| Value::List(homogenise(items))),
                proptest::collection::vec((arb_key(utf8), inner), 0..4)
                    .prop_map(|entries| Value::Compound(entries.into_iter().collect::<Compound>())),
            ]
        })
        .boxed()
}

/// Exact equality, unlike `PartialEq`: compares floats by bit pattern so `NaN`
/// matches itself and `-0.0` does not match `0.0`.
fn bit_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits(),
        (Value::Double(x), Value::Double(y)) => x.to_bits() == y.to_bits(),
        (Value::List(x), Value::List(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| bit_eq(a, b))
        }
        (Value::Compound(x), Value::Compound(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|((ka, va), (kb, vb))| ka == kb && bit_eq(va, vb))
        }
        _ => a == b,
    }
}

/// Like [`bit_eq`], but treats any two NaNs as equal. SNBT renders a float as
/// text, and the text `NaN` carries no payload bits — see
/// `snbt_grammar::nan_payload_bits_do_not_survive_snbt`.
#[cfg(feature = "snbt")]
fn snbt_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Float(x), Value::Float(y)) if x.is_nan() && y.is_nan() => true,
        (Value::Double(x), Value::Double(y)) if x.is_nan() && y.is_nan() => true,
        (Value::List(x), Value::List(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| snbt_eq(a, b))
        }
        (Value::Compound(x), Value::Compound(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|((ka, va), (kb, vb))| ka == kb && snbt_eq(va, vb))
        }
        _ => bit_eq(a, b),
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// The core codec guarantee, over generated documents: whatever goes in comes
    /// back out bit-for-bit, in every one of the three variants.
    #[test]
    fn any_value_roundtrips_in_every_variant(v in arb_value(false)) {
        let be = to_be_bytes(&v).unwrap();
        prop_assert!(bit_eq(&from_be_bytes::<Value>(&mut be.as_slice()).unwrap(), &v));

        let le = to_le_bytes(&v).unwrap();
        prop_assert!(bit_eq(&from_le_bytes::<Value>(&mut le.as_slice()).unwrap(), &v));

        let var = to_varint_bytes(&v).unwrap();
        prop_assert!(bit_eq(&from_varint_bytes::<Value>(&mut var.as_slice()).unwrap(), &v));
    }

    /// Re-encoding a decoded document must reproduce the original bytes exactly.
    /// A round-trip that is merely *value*-preserving would still allow the
    /// encoder to drift (padding, reordering, normalising a length prefix).
    #[test]
    fn encoding_is_a_byte_level_fixpoint(v in arb_value(false)) {
        macro_rules! check {
            ($to:ident, $from:ident) => {{
                let bytes = $to(&v).unwrap();
                let back: Value = $from(&mut bytes.as_slice()).unwrap();
                prop_assert_eq!($to(&back).unwrap(), bytes);
            }};
        }
        for_each_endian!(check);
    }

    /// The three variants are three encodings of *one* document, so decoding any
    /// of them must yield the identical `Value`. This is the property that would
    /// break if, say, the varint reader zigzagged a length prefix that the
    /// fixed-width reader treats as unsigned.
    #[test]
    fn all_three_variants_decode_to_the_same_value(v in arb_value(false)) {
        let be: Value = from_be_bytes(&mut to_be_bytes(&v).unwrap().as_slice()).unwrap();
        let le: Value = from_le_bytes(&mut to_le_bytes(&v).unwrap().as_slice()).unwrap();
        let var: Value = from_varint_bytes(&mut to_varint_bytes(&v).unwrap().as_slice()).unwrap();
        prop_assert!(bit_eq(&be, &le));
        prop_assert!(bit_eq(&le, &var));
    }

    /// Transcoding a document through the other two variants and back must be
    /// lossless: the final big-endian bytes equal the original big-endian bytes.
    #[test]
    fn transcoding_between_variants_is_lossless(v in arb_value(false)) {
        let be = to_be_bytes(&v).unwrap();
        let via_le: Value = from_be_bytes(&mut be.as_slice()).unwrap();
        let le = to_le_bytes(&via_le).unwrap();
        let via_var: Value = from_le_bytes(&mut le.as_slice()).unwrap();
        let var = to_varint_bytes(&via_var).unwrap();
        let back: Value = from_varint_bytes(&mut var.as_slice()).unwrap();
        prop_assert_eq!(to_be_bytes(&back).unwrap(), be);
    }

    /// Cutting a valid document short must always be an error, at every offset.
    /// `malformed_input::every_truncation_point_errors` does this for one
    /// hand-written document; this does it for hundreds of generated ones, so it
    /// reaches truncation points inside varints, inside multi-byte string
    /// payloads and between a list's element type and its length.
    #[test]
    fn every_truncation_of_any_document_errors(v in arb_value(false)) {
        let bytes = to_be_bytes(&v).unwrap();
        for cut in 0..bytes.len() {
            prop_assert!(
                from_be_bytes::<Value>(&mut &bytes[..cut]).is_err(),
                "truncation at {} of {} must error", cut, bytes.len()
            );
        }
    }

    /// Flipping one bit of a valid document must never panic. Corruption is the
    /// realistic failure mode for NBT on disk, and a decoder that aborts on it is
    /// unusable for recovery tooling.
    #[test]
    fn single_bit_corruption_never_panics(v in arb_value(false), seed in any::<u16>()) {
        let mut bytes = to_be_bytes(&v).unwrap();
        if bytes.is_empty() {
            return Ok(());
        }
        let idx = seed as usize % bytes.len();
        bytes[idx] ^= 1 << (seed as usize / 8 % 8);
        let _ = from_be_bytes::<Value>(&mut bytes.as_slice());
        let _ = from_le_bytes::<Value>(&mut bytes.as_slice());
        let _ = from_varint_bytes::<Value>(&mut bytes.as_slice());
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1024, ..ProptestConfig::default() })]

    /// Wholly arbitrary bytes must never panic in any variant. This is the
    /// smoke-test form of fuzzing: NBT is parsed straight off a socket, so a
    /// panic on attacker-chosen bytes is a remote denial of service.
    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..96)) {
        let _ = from_be_bytes::<Value>(&mut bytes.as_slice());
        let _ = from_le_bytes::<Value>(&mut bytes.as_slice());
        let _ = from_varint_bytes::<Value>(&mut bytes.as_slice());
    }

    /// The same for the reflection path, which walks a completely different set
    /// of readers than the dynamic `Value` one.
    #[test]
    fn arbitrary_bytes_never_panic_for_a_derived_struct(
        bytes in proptest::collection::vec(any::<u8>(), 0..96)
    ) {
        #[derive(facet::Facet, Debug)]
        #[facet(nbtx::allow_unknown_fields)]
        struct S {
            a: i32,
            b: Option<String>,
            c: Vec<i64>,
        }
        let _ = from_be_bytes::<S>(&mut bytes.as_slice());
        let _ = from_le_bytes::<S>(&mut bytes.as_slice());
        let _ = from_varint_bytes::<S>(&mut bytes.as_slice());
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    /// A generated struct — not a `Value` — through the facet reflection path, in
    /// all three variants. The two paths share no code below the entry points.
    #[test]
    fn any_derived_struct_roundtrips(
        b in any::<i8>(),
        s in any::<i16>(),
        i in any::<i32>(),
        l in any::<i64>(),
        f in any::<u32>(),
        d in any::<u64>(),
        text in arb_utf8_string(10),
        bytes in proptest::collection::vec(any::<u8>(), 0..10),
        ints in proptest::collection::vec(any::<i32>(), 0..6),
        opt in proptest::option::of(any::<i64>()),
    ) {
        #[derive(facet::Facet, Debug)]
        struct S {
            b: i8,
            s: i16,
            i: i32,
            l: i64,
            f: f32,
            d: f64,
            text: String,
            bytes: Vec<u8>,
            ints: Vec<i32>,
            opt: Option<i64>,
        }
        let v = S {
            b, s, i, l,
            f: f32::from_bits(f),
            d: f64::from_bits(d),
            text, bytes, ints, opt,
        };
        macro_rules! check {
            ($to:ident, $from:ident) => {{
                let enc = $to(&v).unwrap();
                let back: S = $from(&mut enc.as_slice()).unwrap();
                prop_assert_eq!(back.b, v.b);
                prop_assert_eq!(back.s, v.s);
                prop_assert_eq!(back.i, v.i);
                prop_assert_eq!(back.l, v.l);
                prop_assert_eq!(back.f.to_bits(), v.f.to_bits());
                prop_assert_eq!(back.d.to_bits(), v.d.to_bits());
                prop_assert_eq!(&back.text, &v.text);
                prop_assert_eq!(&back.bytes, &v.bytes);
                prop_assert_eq!(&back.ints, &v.ints);
                prop_assert_eq!(back.opt, v.opt);
                // The struct path must agree with the wire byte-for-byte too.
                prop_assert_eq!($to(&back).unwrap(), enc);
            }};
        }
        for_each_endian!(check);
    }

    /// On-disk compound key order must survive a round-trip for arbitrary key
    /// sets, not just the three-key case pinned in `tag_semantics`.
    #[cfg(feature = "preserve_order")]
    #[test]
    fn compound_key_order_survives_any_key_set(
        keys in proptest::collection::vec(arb_utf8_string(5), 0..12)
    ) {
        let mut map = Compound::new();
        for (i, k) in keys.iter().enumerate() {
            map.insert(BString::from(k.as_str()), Value::Int(i as i32));
        }
        let original: Vec<BString> = map.keys().cloned().collect();
        let doc = Value::Compound(map);
        let bytes = to_be_bytes(&doc).unwrap();
        let back: Value = from_be_bytes(&mut bytes.as_slice()).unwrap();
        let got: Vec<BString> = back.as_compound().unwrap().keys().cloned().collect();
        prop_assert_eq!(got, original);
    }
}

#[cfg(feature = "snbt")]
proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// SNBT is a second, textual encoding of the same model, so it must preserve
    /// the same documents the binary codec does. Strings are restricted to UTF-8
    /// because SNBT is text and renders raw bytes lossily by design.
    #[test]
    fn any_utf8_value_roundtrips_through_snbt(v in arb_value(true)) {
        let text = nbtx::to_string(&v).unwrap();
        let back: Value = nbtx::from_string(&text)
            .unwrap_or_else(|e| panic!("failed to re-parse {text:?}: {e}"));
        prop_assert!(snbt_eq(&back, &v), "{:?} != {:?} (via {})", back, v, text);
    }

    /// Rendering a re-parsed document must produce the identical text — the
    /// textual counterpart of `encoding_is_a_byte_level_fixpoint`.
    #[test]
    fn snbt_rendering_is_a_text_level_fixpoint(v in arb_value(true)) {
        let text = nbtx::to_string(&v).unwrap();
        let back: Value = nbtx::from_string(&text).unwrap();
        prop_assert_eq!(nbtx::to_string(&back).unwrap(), text);
    }

    /// The two codecs must agree on the same document: going through SNBT and
    /// going through big-endian bytes must land on the same `Value`.
    #[test]
    fn the_binary_and_textual_codecs_agree(v in arb_value(true)) {
        let via_binary: Value =
            from_be_bytes(&mut to_be_bytes(&v).unwrap().as_slice()).unwrap();
        let via_snbt: Value = nbtx::from_string(nbtx::to_string(&v).unwrap()).unwrap();
        prop_assert!(snbt_eq(&via_binary, &via_snbt));
    }

    /// Arbitrary text must never panic the recursive-descent parser.
    #[test]
    fn arbitrary_text_never_panics_the_snbt_parser(text in arb_utf8_string(40)) {
        let _: Result<Value, _> = nbtx::from_string(&text);
    }
}
