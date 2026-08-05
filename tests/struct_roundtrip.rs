//! Integration tests for the `#[derive(Facet)]` struct (de)serialization path.

#![cfg(feature = "nbt")]

use bstr::BString;
use facet::Facet;
use nbtx::Compound;
use nbtx::{
    Value, from_be_bytes, from_le_bytes, from_varint_bytes, to_be_bytes, to_le_bytes,
    to_varint_bytes,
};

const BIG_TEST_NBT: &[u8] = include_bytes!("fixtures/bigtest.nbt");
const PLAYER_NAN_VALUE_NBT: &[u8] = include_bytes!("fixtures/player_nan_value.nbt");
const SERVERS_DAT: &[u8] = include_bytes!("../examples/servers.dat");

/// Invokes a caller-defined `check!($to, $from)` macro once for each of the
/// three endianness function pairs. Passing the function identifiers (rather
/// than binding them to a `let`) keeps every call an independent generic
/// instantiation, so `check!` may deserialize into several different types.
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

#[test]
fn read_write_option() {
    #[derive(Facet, Debug, PartialEq)]
    struct Optional {
        optional: Option<i32>,
        required: String,
    }

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            // A `None` field is omitted on the wire and restored as `None`.
            let none = Optional {
                optional: None,
                required: "This is None".to_owned(),
            };
            let bytes = $to(&none).unwrap();
            let back: Optional = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, none);

            // A `Some` field round-trips as the inner value.
            let some = Optional {
                optional: Some(42),
                required: "This is Some".to_owned(),
            };
            let bytes = $to(&some).unwrap();
            let back: Optional = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, some);
        }};
    }
    for_each_endian!(check);
}

/// Exercises the full type-to-tag convention: `String`, `Vec<u8>` (`ByteArray`),
/// `Vec<i32>` (`IntArray`), `Vec<i64>` (`LongArray`), a generic `Vec<T>`
/// (`List`), a nested struct (`Compound`) and a dynamic `Value` field.
#[test]
fn derive_tag_convention() {
    #[derive(Facet, Debug, PartialEq)]
    struct Inner {
        n: i32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct AllKinds {
        flag: bool,
        b: i8,
        s: i16,
        i: i32,
        l: i64,
        f: f32,
        d: f64,
        text: String,
        bytes: Vec<u8>,
        ints: Vec<i32>,
        longs: Vec<i64>,
        list: Vec<String>,
        nested: Inner,
        dynamic: Value,
    }

    let data = AllKinds {
        flag: true,
        b: -1,
        s: 2,
        i: 3,
        l: 4,
        f: 1.5,
        d: 2.5,
        text: "hello".to_owned(),
        bytes: vec![0, 0x7f, 0x80, 0xff],
        ints: vec![1, -2, 3],
        longs: vec![10, 20],
        list: vec!["a".to_owned(), "bb".to_owned()],
        nested: Inner { n: 99 },
        dynamic: Value::Compound(Compound::from([("x".into(), Value::Byte(7))])),
    };

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&data).unwrap();
            let back: AllKinds = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, data);

            // Verify each field maps to the intended NBT tag via a `Value` decode.
            let value: Value = $from(&mut bytes.as_slice()).unwrap();
            let c = value.as_compound().unwrap();
            assert!(c[&BString::from("flag")].is_byte());
            assert!(c[&BString::from("bytes")].is_byte_array());
            assert!(c[&BString::from("ints")].is_int_array());
            assert!(c[&BString::from("longs")].is_long_array());
            assert!(c[&BString::from("list")].is_list());
            assert!(c[&BString::from("nested")].is_compound());
            assert!(c[&BString::from("dynamic")].is_compound());
        }};
    }
    for_each_endian!(check);
}

/// A bare `bstr::BString` field carries an NBT `String` tag (not a `ByteArray`),
/// and its raw, non-UTF-8 bytes round-trip losslessly.
#[test]
fn bstring_field_roundtrips_as_string() {
    #[derive(Facet, Debug, PartialEq)]
    struct Holder {
        text: BString,
    }

    let raw = BString::from(vec![0x68, 0xff, 0xfe, 0x00, 0x69]);
    assert!(std::str::from_utf8(raw.as_ref()).is_err());
    let holder = Holder { text: raw.clone() };

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&holder).unwrap();
            let back: Holder = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, holder);

            // The field must decode as a `String` tag (8), not a `ByteArray` (7).
            let value: Value = $from(&mut bytes.as_slice()).unwrap();
            let field = &value.as_compound().unwrap()[&BString::from("text")];
            assert!(field.is_string(), "BString field must be a String tag");
            assert_eq!(field.as_string().unwrap(), &raw);
        }};
    }
    for_each_endian!(check);
}

#[test]
fn read_write_bigtest_struct() {
    #[derive(Facet, Debug, PartialEq)]
    struct Food {
        name: String,
        value: f32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Nested {
        egg: Food,
        ham: Food,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct ListCompound {
        #[facet(rename = "created-on")]
        created_on: i64,
        name: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct AllTypes {
        #[facet(rename = "nested compound test")]
        nested: Nested,
        #[facet(rename = "intTest")]
        int_test: i32,
        #[facet(rename = "byteTest")]
        byte_test: i8,
        #[facet(rename = "stringTest")]
        string_test: String,
        #[facet(rename = "listTest (long)")]
        long_list_test: [i64; 5],
        #[facet(rename = "doubleTest")]
        double_test: f64,
        #[facet(rename = "floatTest")]
        float_test: f32,
        #[facet(rename = "longTest")]
        long_test: i64,
        #[facet(rename = "listTest (compound)")]
        compound_list_test: [ListCompound; 2],
        #[facet(
            rename = "byteArrayTest (the first 1000 values of (n*n*255+n*7)%100, starting with n=0 (0, 62, 34, 16, 8, ...))"
        )]
        byte_array_test: Vec<u8>,
        #[facet(rename = "shortTest")]
        short_test: i16,
    }

    let decoded: AllTypes = from_be_bytes(&mut BIG_TEST_NBT.to_vec().as_slice()).unwrap();

    // Self round-trip through the struct path.
    let encoded = to_be_bytes(&decoded).unwrap();
    let decoded2: AllTypes = from_be_bytes(&mut encoded.as_slice()).unwrap();
    assert_eq!(decoded, decoded2);

    // Cross-endianness round-trip through `Value`.
    let value: Value = from_be_bytes(&mut BIG_TEST_NBT.to_vec().as_slice()).unwrap();
    let net = to_varint_bytes(&value).unwrap();
    let value2: Value = from_varint_bytes(&mut net.as_slice()).unwrap();
    assert_eq!(value, value2);
}

#[test]
fn read_write_player() {
    // The fixture also carries `Inventory`, which this struct does not model.
    // Unknown keys are rejected by default, so opt this struct out explicitly.
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename_all = "PascalCase")]
    #[facet(nbtx::allow_unknown_fields)]
    struct Player {
        pos: [f64; 3],
        motion: [f64; 3],
        on_ground: bool,
        death_time: i16,
        air: i16,
        health: i16,
        fall_distance: f32,
        attack_time: i16,
        hurt_time: i16,
        fire: i16,
        rotation: [f32; 2],
    }

    // The fixture stores a NaN, so comparing decoded `Player`s directly would
    // fail (NaN != NaN); compare the re-encoded bytes instead, which preserves
    // the NaN bit pattern.
    let decoded: Player = from_be_bytes(&mut PLAYER_NAN_VALUE_NBT.to_vec().as_slice()).unwrap();
    let encoded = to_be_bytes(&decoded).unwrap();
    let decoded2: Player = from_be_bytes(&mut encoded.as_slice()).unwrap();
    let encoded2 = to_be_bytes(&decoded2).unwrap();
    assert_eq!(encoded, encoded2);
}

#[test]
fn servers_dat_struct_decode() {
    #[derive(Facet, Debug, Clone)]
    struct ServerDat {
        servers: Vec<ServerDatItem>,
    }

    // `servers.dat` entries carry a `hidden` key this struct does not model.
    #[derive(Facet, Debug, Clone)]
    #[facet(rename_all = "camelCase")]
    #[facet(nbtx::allow_unknown_fields)]
    struct ServerDatItem {
        icon: Option<String>,
        ip: String,
        name: String,
        accept_textures: Option<bool>,
    }

    let server_dat: ServerDat = from_be_bytes(&mut SERVERS_DAT.to_vec().as_slice()).unwrap();
    assert!(!server_dat.servers.is_empty());
}

/// A unit enum field must round-trip: it's written as a String tag holding the
/// variant's name, and reading that tag back must select the matching variant
/// rather than rejecting all enums outright.
#[test]
fn unit_enum_field_roundtrips() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    #[repr(u8)]
    enum Mode {
        Survival,
        Creative,
        Adventure,
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    struct Settings {
        mode: Mode,
    }

    let value = Settings {
        mode: Mode::Creative,
    };

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&value).unwrap();
            let back: Settings = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, value);

            // The variant is written as a String tag holding its name.
            let dynamic: Value = $from(&mut bytes.as_slice()).unwrap();
            let mode = dynamic
                .as_compound()
                .unwrap()
                .get(&BString::from("mode"))
                .unwrap();
            assert_eq!(mode.as_string().unwrap(), "Creative");
        }};
    }

    for_each_endian!(check);
}

/// A `bool` field is `true` only for the byte `0x01`. Every other byte,
/// including `0x02`, is `false`: TAG_Byte is a signed 8-bit integer that happens
/// to be used as a flag, and treating "any non-zero" as `true` would disagree
/// with the Bedrock decoders that produced the data.
#[test]
fn bool_is_true_only_for_byte_1() {
    #[derive(Facet, Debug)]
    struct S {
        #[facet(rename = "a")]
        a: bool,
    }
    let bytes = hex(concat!("0a0000", "01", "010061", "02", "00"));
    let s: S = from_le_bytes(&mut bytes.as_slice()).unwrap();
    assert!(!s.a, "TAG_Byte 0x02 must decode to false");
}

/// A compound key with no matching struct field is an error by default. Silently
/// dropping it would discard data the caller never learns was there, so schema
/// drift surfaces at decode time instead.
#[test]
fn unknown_compound_key_is_rejected_when_decoding_into_a_struct() {
    #[derive(Facet, Debug)]
    struct S {
        #[facet(rename = "a")]
        a: i32,
    }
    let bytes = hex(concat!(
        "0a0000", "03010061", "01000000", "03010062", "02000000", "00"
    ));
    let err = from_le_bytes::<S>(&mut bytes.as_slice())
        .expect_err("the unmatched key 'b' must be rejected");
    match err {
        nbtx::Error::UnknownField(e) => {
            assert_eq!(e.field(), "b");
            assert_eq!(e.container(), "S");
        }
        other => panic!("expected UnknownField, got {other:?}"),
    }
}

/// The opt-out: an otherwise identical struct carrying
/// `#[facet(nbtx::allow_unknown_fields)]` silently skips the unmatched key,
/// which is what nbtx did unconditionally before 4.0. Deny is the default; this
/// attribute is the only way back to lenient decoding.
#[test]
fn unknown_compound_key_is_skipped_with_allow_unknown_fields() {
    #[derive(Facet, Debug)]
    #[facet(nbtx::allow_unknown_fields)]
    struct S {
        #[facet(rename = "a")]
        a: i32,
    }
    let bytes = hex(concat!(
        "0a0000", "03010061", "01000000", "03010062", "02000000", "00"
    ));
    let s: S = from_le_bytes(&mut bytes.as_slice())
        .expect("allow_unknown_fields must skip the unmatched key 'b'");
    assert_eq!(s.a, 1);
}

/// The skip must consume the *whole* unknown payload, however deeply nested, so
/// the rest of the compound still decodes.
#[test]
fn allow_unknown_fields_skips_nested_payloads() {
    #[derive(Facet, Debug)]
    #[facet(nbtx::allow_unknown_fields)]
    struct S {
        #[facet(rename = "a")]
        a: i32,
    }
    // root; Compound "u" { List "l" of two Compounds each with Int "z" }; Int "a"=1; end
    let bytes = hex(concat!(
        "0a0000", "0a", "010075", // Compound "u"
        "09", "01006c", "0a", "02000000", // List "l" of 2 Compounds
        "03", "01007a", "07000000", "00", // { z: 7 }
        "03", "01007a", "08000000", "00", // { z: 8 }
        "00", // end of "u"
        "03", "010061", "01000000", // Int "a" = 1
        "00"        // end of root
    ));
    let s: S = from_le_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(s.a, 1, "the key after the skipped one must still decode");
}

/// The first-wins rule for duplicate compound keys (see
/// `tag_semantics::duplicate_compound_keys_keep_the_first_value`) must hold on
/// the derived-struct and map paths too — each reads keys in its own loop.
#[test]
fn duplicate_keys_first_wins_for_structs_and_maps() {
    use std::collections::HashMap;

    #[derive(Facet, Debug)]
    struct S {
        d: i32,
    }

    // root; Int "d"=1; Int "d"=2; end
    let bytes = hex(concat!(
        "0a0000", "03", "0001", "64", "00000001", "03", "0001", "64", "00000002", "00"
    ));

    let s: S = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(s.d, 1, "struct decode must keep the first duplicate");

    let m: HashMap<String, i32> = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(
        m.get("d"),
        Some(&1),
        "map decode must keep the first duplicate"
    );
}
