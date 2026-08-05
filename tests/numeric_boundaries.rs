//! Boundary values for the six numeric tags, and the encodings they produce.
//!
//! `tag_semantics` checks that the integer extremes survive a round-trip and
//! `wire_format` pins a handful of golden vectors; this file goes at the numbers
//! themselves — every value where an encoder is likely to be off by one, to
//! widen, to normalise, or to change its byte width:
//!
//! * every `i8`, and the `i16`/`i32`/`i64` extremes together with `0` and `-1`
//!   (the two values whose zigzag encodings are most easily transposed);
//! * each 7-bit boundary of the varint encoding, asserted as a byte *count*, so
//!   a varint that silently grew or shrank is caught;
//! * the float values `PartialEq` cannot distinguish — `NaN` (which is not equal
//!   to itself), `-0.0` (which *is* equal to `0.0` despite different bytes), and
//!   subnormals (which a naive conversion through `f64` would flush to zero).
//!   These are all compared by bit pattern, never by `==`.

#![cfg(feature = "nbt")]

use bstr::BString;
use nbtx::{
    Compound, Value, from_be_bytes, from_le_bytes, from_varint_bytes, to_be_bytes, to_le_bytes,
    to_varint_bytes,
};

macro_rules! for_each_endian {
    ($check:ident) => {
        $check!(to_be_bytes, from_be_bytes);
        $check!(to_le_bytes, from_le_bytes);
        $check!(to_varint_bytes, from_varint_bytes);
    };
}

fn hex(s: &str) -> Vec<u8> {
    let packed: String = s.split_whitespace().collect();
    assert!(packed.len().is_multiple_of(2), "odd-length hex: {s}");
    (0..packed.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&packed[i..i + 2], 16).unwrap())
        .collect()
}

fn root(key: &str, v: Value) -> Value {
    Value::Compound(Compound::from([(BString::from(key), v)]))
}

fn get<'a>(v: &'a Value, key: &str) -> &'a Value {
    v.as_compound().unwrap().get(&BString::from(key)).unwrap()
}

/// The number of payload bytes a varint-variant scalar root occupies: total
/// length minus the two-byte header (tag byte + an empty name's `0x00` varuint
/// length prefix).
fn varint_payload_len(v: &Value) -> usize {
    to_varint_bytes(v).unwrap().len() - 2
}

/// Every `i8` value must survive, not just the extremes: `Value::Byte` is the
/// one tag whose entire domain is small enough to enumerate, so there is no
/// excuse for sampling it.
#[test]
fn byte_tag_covers_every_i8_value() {
    for n in i8::MIN..=i8::MAX {
        let doc = root("b", Value::Byte(n));
        macro_rules! check {
            ($to:ident, $from:ident) => {{
                let bytes = $to(&doc).unwrap();
                let back: Value = $from(&mut bytes.as_slice()).unwrap();
                assert_eq!(get(&back, "b"), &Value::Byte(n), "byte {n}");
            }};
        }
        for_each_endian!(check);
        // TAG_Byte is a single raw byte in all three variants, sign included.
        assert_eq!(
            *to_be_bytes(&Value::Byte(n)).unwrap().last().unwrap(),
            n.cast_unsigned()
        );
    }
}

/// `Short` stays a fixed-width little-endian 16-bit value in the *varint*
/// variant — only `Int`/`Long` and the length prefixes become varints. The
/// extremes make a byte-swap visible, which `Short(300)` in `wire_format` does
/// not for `0` or `-1`.
#[test]
fn short_boundaries_are_byte_exact_in_every_variant() {
    for (n, be, le) in [
        (0i16, "02 00 00 00 00", "02 00 00 00 00"),
        (-1, "02 00 00 ff ff", "02 00 00 ff ff"),
        (i16::MIN, "02 00 00 80 00", "02 00 00 00 80"),
        (i16::MAX, "02 00 00 7f ff", "02 00 00 ff 7f"),
    ] {
        let v = Value::Short(n);
        assert_eq!(to_be_bytes(&v).unwrap(), hex(be), "BE short {n}");
        assert_eq!(to_le_bytes(&v).unwrap(), hex(le), "LE short {n}");
        // The varint header is one byte shorter (varuint name length), but the
        // payload is the same little-endian pair.
        assert_eq!(
            to_varint_bytes(&v).unwrap()[2..],
            hex(le)[3..],
            "varint short {n} must reuse the little-endian payload"
        );
    }
}

/// `0` and `-1` are the two values a zigzag implementation most easily
/// transposes (`zigzag(0) == 0`, `zigzag(-1) == 1`), and `wire_format` pins only
/// `-1`. Asserting both, for `Int` and `Long`, catches an inverted sign bit.
#[test]
fn zero_and_minus_one_zigzag_in_the_expected_direction() {
    assert_eq!(to_varint_bytes(&Value::Int(0)).unwrap(), hex("03 00 00"));
    assert_eq!(to_varint_bytes(&Value::Int(-1)).unwrap(), hex("03 00 01"));
    assert_eq!(to_varint_bytes(&Value::Int(1)).unwrap(), hex("03 00 02"));
    assert_eq!(to_varint_bytes(&Value::Long(0)).unwrap(), hex("04 00 00"));
    assert_eq!(to_varint_bytes(&Value::Long(-1)).unwrap(), hex("04 00 01"));
    assert_eq!(to_varint_bytes(&Value::Long(1)).unwrap(), hex("04 00 02"));
    // ...and the fixed-width variants keep the two's-complement pattern.
    assert_eq!(
        to_be_bytes(&Value::Int(-1)).unwrap(),
        hex("03 00 00 ffffffff")
    );
    assert_eq!(
        to_be_bytes(&Value::Int(0)).unwrap(),
        hex("03 00 00 00000000")
    );
}

/// A varint's *width* is the property that silently regresses: a wrong shift
/// still round-trips through nbtx's own reader while producing bytes no other
/// implementation accepts. These are the exact values at which the zigzagged
/// payload crosses each seven-bit boundary.
#[test]
fn int_varints_change_width_at_each_seven_bit_boundary() {
    for (n, want) in [
        (0i32, 1usize),
        (63, 1),   // zigzag 126, still one byte
        (64, 2),   // zigzag 128, the first two-byte value
        (-64, 1),  // zigzag 127
        (-65, 2),  // zigzag 129
        (8191, 2), // zigzag 16382
        (8192, 3), // zigzag 16384
        (1_048_575, 3),
        (1_048_576, 4),
        (134_217_727, 4),
        (134_217_728, 5),
        (i32::MAX, 5),
        (i32::MIN, 5),
    ] {
        assert_eq!(
            varint_payload_len(&Value::Int(n)),
            want,
            "Int({n}) must occupy {want} varint bytes"
        );
    }
}

/// The 64-bit equivalent, up to the 10-byte cap that
/// `security_limits::maximum_width_varints_are_accepted` pins from the read
/// side.
#[test]
fn long_varints_change_width_at_each_seven_bit_boundary() {
    for (n, want) in [
        (0i64, 1usize),
        (63, 1),
        (64, 2),
        (1_048_575, 3),
        (1_048_576, 4),
        (i64::from(i32::MAX), 5),
        (34_359_738_367, 6),
        (34_359_738_368, 6),
        (i64::MAX, 10),
        (i64::MIN, 10),
    ] {
        assert_eq!(
            varint_payload_len(&Value::Long(n)),
            want,
            "Long({n}) must occupy {want} varint bytes"
        );
    }
}

/// String lengths are *unsigned* varints, so their width boundaries are at
/// `2^7`/`2^14`, not at the zigzagged `2^6`/`2^13` of a signed one. Getting this
/// wrong doubles the cost of every short string without breaking a self
/// round-trip.
#[test]
fn string_length_prefixes_change_width_at_unsigned_boundaries() {
    for (len, want_prefix) in [
        (0usize, 1usize),
        (127, 1),
        (128, 2),
        (16_383, 2),
        (16_384, 3),
    ] {
        let v = Value::String(BString::from(vec![b'x'; len]));
        assert_eq!(
            varint_payload_len(&v) - len,
            want_prefix,
            "a {len}-byte string must carry a {want_prefix}-byte length prefix"
        );
    }
}

/// Array and list lengths go through the *signed* `i32` path instead, so their
/// boundaries are the zigzagged ones. The contrast with the string prefix above
/// is the point.
#[test]
fn array_length_prefixes_change_width_at_signed_boundaries() {
    for (len, want_prefix) in [(0usize, 1usize), (63, 1), (64, 2), (8_191, 2), (8_192, 3)] {
        let v = Value::ByteArray(vec![0u8; len]);
        assert_eq!(
            varint_payload_len(&v) - len,
            want_prefix,
            "a {len}-element ByteArray must carry a {want_prefix}-byte length prefix"
        );
    }
}

/// Every `f32` the codec must not normalise, compared **by bit pattern**: `==`
/// would pass `-0.0` off as `0.0` and would fail outright on `NaN`.
#[test]
fn float_special_values_roundtrip_bit_exactly() {
    let cases: [(&str, f32); 12] = [
        ("zero", 0.0),
        ("negative zero", -0.0),
        ("one", 1.0),
        ("negative one", -1.0),
        ("epsilon", f32::EPSILON),
        ("min positive normal", f32::MIN_POSITIVE),
        ("smallest subnormal", f32::from_bits(1)),
        ("largest subnormal", f32::from_bits(0x007f_ffff)),
        ("max", f32::MAX),
        ("min", f32::MIN),
        ("infinity", f32::INFINITY),
        ("negative infinity", f32::NEG_INFINITY),
    ];
    for (label, f) in cases {
        let doc = root("f", Value::Float(f));
        macro_rules! check {
            ($to:ident, $from:ident) => {{
                let bytes = $to(&doc).unwrap();
                let back: Value = $from(&mut bytes.as_slice()).unwrap();
                assert_eq!(
                    get(&back, "f").as_float().unwrap().to_bits(),
                    f.to_bits(),
                    "{label} ({}): bit pattern changed",
                    stringify!($to)
                );
            }};
        }
        for_each_endian!(check);
    }
}

/// The `f64` equivalent. `tag_semantics::double_value_roundtrip` covers four
/// ordinary values on the little-endian path only, and compares with `==`.
#[test]
fn double_special_values_roundtrip_bit_exactly() {
    let cases: [(&str, f64); 12] = [
        ("zero", 0.0),
        ("negative zero", -0.0),
        ("one", 1.0),
        ("negative one", -1.0),
        ("epsilon", f64::EPSILON),
        ("min positive normal", f64::MIN_POSITIVE),
        ("smallest subnormal", f64::from_bits(1)),
        ("largest subnormal", f64::from_bits(0x000f_ffff_ffff_ffff)),
        ("max", f64::MAX),
        ("min", f64::MIN),
        ("infinity", f64::INFINITY),
        ("negative infinity", f64::NEG_INFINITY),
    ];
    for (label, d) in cases {
        let doc = root("d", Value::Double(d));
        macro_rules! check {
            ($to:ident, $from:ident) => {{
                let bytes = $to(&doc).unwrap();
                let back: Value = $from(&mut bytes.as_slice()).unwrap();
                assert_eq!(
                    get(&back, "d").as_double().unwrap().to_bits(),
                    d.to_bits(),
                    "{label} ({}): bit pattern changed",
                    stringify!($to)
                );
            }};
        }
        for_each_endian!(check);
    }
}

/// `-0.0 == 0.0` in Rust, so a `PartialEq`-based round-trip test cannot tell the
/// two apart. On the wire they differ in exactly one bit, and that difference
/// must be preserved: Minecraft stores signed zeroes in entity motion vectors.
#[test]
fn negative_zero_is_wire_distinct_from_positive_zero() {
    let pos = to_be_bytes(&Value::Float(0.0)).unwrap();
    let neg = to_be_bytes(&Value::Float(-0.0)).unwrap();
    assert_ne!(pos, neg, "-0.0 and 0.0 must not encode identically");
    assert_eq!(neg[3] ^ pos[3], 0x80, "only the sign bit may differ");

    let pos = to_le_bytes(&Value::Double(0.0)).unwrap();
    let neg = to_le_bytes(&Value::Double(-0.0)).unwrap();
    assert_eq!(neg.last().unwrap() ^ pos.last().unwrap(), 0x80);
}

/// A NaN carries a 22/51-bit payload that some conversions (notably going
/// through a wider type and back, or through a text format) quietly canonicalise
/// to a single "default" NaN. The binary codec copies raw bytes, so every
/// payload — quiet, signalling, sign bit set — must come back untouched.
#[test]
fn nan_payload_bits_survive_the_binary_roundtrip() {
    for bits in [
        0x7fc0_0000u32, // canonical quiet NaN
        0xffc0_0000,    // ...with the sign bit set
        0x7f80_0001,    // signalling NaN, minimal payload
        0x7fc0_1234,    // quiet NaN, arbitrary payload
        0x7fff_ffff,    // all payload bits set
    ] {
        let f = f32::from_bits(bits);
        assert!(f.is_nan());
        let doc = root("f", Value::Float(f));
        macro_rules! check {
            ($to:ident, $from:ident) => {{
                let enc = $to(&doc).unwrap();
                let back: Value = $from(&mut enc.as_slice()).unwrap();
                assert_eq!(
                    get(&back, "f").as_float().unwrap().to_bits(),
                    bits,
                    "f32 NaN payload {bits:#010x} was canonicalised"
                );
            }};
        }
        for_each_endian!(check);
    }

    for bits in [
        0x7ff8_0000_0000_0000u64,
        0xfff8_0000_0000_0000,
        0x7ff0_dead_beef_0001,
    ] {
        let d = f64::from_bits(bits);
        assert!(d.is_nan());
        let doc = root("d", Value::Double(d));
        let enc = to_be_bytes(&doc).unwrap();
        let back: Value = from_be_bytes(&mut enc.as_slice()).unwrap();
        assert_eq!(
            get(&back, "d").as_double().unwrap().to_bits(),
            bits,
            "f64 NaN payload {bits:#018x} was canonicalised"
        );
    }
}

/// The varint variant leaves `Float`/`Double` fixed-width little-endian; only
/// `Int`/`Long` and length prefixes become varints. Asserted as a byte-level
/// *equality* with the little-endian payload for a value whose bytes would be
/// unmistakably different if it had been varint-encoded.
#[test]
fn floats_are_never_varint_encoded() {
    for v in [
        Value::Float(f32::MAX),
        Value::Float(-0.0),
        Value::Double(f64::MIN),
        Value::Double(f64::EPSILON),
    ] {
        let le = to_le_bytes(&v).unwrap();
        let var = to_varint_bytes(&v).unwrap();
        // Headers differ (a `u16` vs a varuint name-length prefix), payloads do not.
        assert_eq!(
            &var[2..],
            &le[3..],
            "{v:?}: varint payload must equal the LE one"
        );
    }
}

/// An `f32` must not be widened to `f64` and narrowed back: `0.1f32` and
/// `0.1f64` are different numbers, and confusing the two is the classic
/// float-precision codec bug.
#[test]
fn float_precision_is_not_widened_through_double() {
    let doc = root("f", Value::Float(0.1));
    let bytes = to_be_bytes(&doc).unwrap();
    assert_eq!(
        bytes.len(),
        3 + 1 + 2 + 1 + 4 + 1,
        "a Float payload must be four bytes, not eight"
    );
    let back: Value = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(
        get(&back, "f").as_float().unwrap().to_bits(),
        0.1f32.to_bits()
    );
    // The same decimal as a Double is a genuinely different bit pattern.
    assert_ne!(f64::from(0.1f32).to_bits(), 0.1f64.to_bits());
}

/// Boundary values must survive *inside* containers too — the array writers use
/// separate loops from the scalar ones, so a widening bug could hide there.
#[test]
fn boundary_values_survive_inside_typed_arrays() {
    let ints = vec![i32::MIN, -1, 0, 1, i32::MAX];
    let longs = vec![i64::MIN, -1, 0, 1, i64::MAX];
    let bytes = vec![0u8, 1, 0x7f, 0x80, 0xff];
    let doc = Value::Compound(Compound::from([
        (BString::from("ia"), Value::IntArray(ints.clone())),
        (BString::from("la"), Value::LongArray(longs.clone())),
        (BString::from("ba"), Value::ByteArray(bytes.clone())),
    ]));

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let enc = $to(&doc).unwrap();
            let back: Value = $from(&mut enc.as_slice()).unwrap();
            assert_eq!(get(&back, "ia").as_int_array().unwrap(), &ints);
            assert_eq!(get(&back, "la").as_long_array().unwrap(), &longs);
            assert_eq!(get(&back, "ba").as_byte_array().unwrap(), &bytes);
        }};
    }
    for_each_endian!(check);
}

/// The same for a `List`, whose elements are written through the generic
/// per-element path rather than the bulk array one.
#[test]
fn boundary_floats_survive_inside_a_list() {
    let floats = [0.0f32, -0.0, f32::MIN_POSITIVE, f32::from_bits(1), f32::NAN];
    let doc = root(
        "l",
        Value::List(floats.iter().copied().map(Value::Float).collect()),
    );

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let enc = $to(&doc).unwrap();
            let back: Value = $from(&mut enc.as_slice()).unwrap();
            let list = get(&back, "l").as_list().unwrap();
            for (got, want) in list.iter().zip(floats.iter()) {
                assert_eq!(got.as_float().unwrap().to_bits(), want.to_bits());
            }
        }};
    }
    for_each_endian!(check);
}

/// A string of exactly `MAX_STRING_LEN` bytes is legal in *all three* variants —
/// `security_limits` pins the accepting side for big-endian only, and the varint
/// variant is the one whose read path carries the explicit ceiling.
#[test]
fn a_string_of_exactly_max_len_encodes_and_decodes_in_every_variant() {
    let s = BString::from(vec![b'z'; nbtx::MAX_STRING_LEN]);
    let doc = root("s", Value::String(s.clone()));

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let enc = $to(&doc).unwrap();
            let back: Value = $from(&mut enc.as_slice()).unwrap();
            assert_eq!(get(&back, "s").as_string().unwrap(), &s);
        }};
    }
    for_each_endian!(check);

    // One byte more is refused everywhere, not only on the big-endian path.
    let over = root(
        "s",
        Value::String(BString::from(vec![b'z'; nbtx::MAX_STRING_LEN + 1])),
    );
    assert!(to_be_bytes(&over).is_err());
    assert!(to_le_bytes(&over).is_err());
    assert!(to_varint_bytes(&over).is_err());
}

/// Integer tags must not be interchangeable: the same numeric value under a
/// different tag is a different document, and each keeps its own width.
#[test]
fn the_same_number_under_different_integer_tags_encodes_differently() {
    let one = [
        (Value::Byte(1), 1usize),
        (Value::Short(1), 2),
        (Value::Int(1), 4),
        (Value::Long(1), 8),
    ];
    for (v, payload_len) in one {
        let bytes = to_be_bytes(&v).unwrap();
        assert_eq!(
            bytes.len() - 3,
            payload_len,
            "{v:?} must have a {payload_len}-byte big-endian payload"
        );
    }
    assert_ne!(
        to_be_bytes(&Value::Int(1)).unwrap(),
        to_be_bytes(&Value::Long(1)).unwrap()
    );
    assert_ne!(Value::Int(1), Value::Long(1));
}
