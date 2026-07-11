#![allow(const_item_mutation)] // We make use of constant mutation on purpose in this test.

use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;

use bstr::BString;
use byteorder::BigEndian;
use serde::{Deserialize, Serialize};

use crate::{
    Error, NbtByteArray, NbtString, NetworkLittleEndian, Value, from_be_bytes, from_le_bytes,
    from_net_bytes,
    nbt::ser::{to_be_bytes, to_bytes, to_le_bytes, to_net_bytes},
};

const BIG_TEST_NBT: &[u8] = include_bytes!("../test/bigtest.nbt");
const HELLO_WORLD_NBT: &[u8] = include_bytes!("../test/hello_world.nbt");
const PLAYER_NAN_VALUE_NBT: &[u8] = include_bytes!("../test/player_nan_value.nbt");

#[test]
fn read_write_option() {
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Optional {
        optional: Option<i32>,
        required: String,
    }

    let some = Optional {
        optional: None,
        required: "This is Some".to_owned(),
    };

    let some_ser = to_be_bytes(&some).unwrap();
    let mut some_ser_slice = Cursor::new(some_ser.as_slice());

    let some_de: Value = from_be_bytes(&mut some_ser_slice).unwrap();
    dbg!(some_de);

    let _none = Optional {
        optional: None,
        required: "This is None".to_owned(),
    };
}

#[test]
fn read_write_all() {
    let value = Value::Compound(BTreeMap::from([
        ("byte".into(), Value::Byte(42)),
        ("short".into(), Value::Short(42)),
        ("int".into(), Value::Int(42)),
        ("long".into(), Value::Long(42)),
        ("float".into(), Value::Float(42.0)),
        ("double".into(), Value::Double(42.0)),
        ("byte_array".into(), Value::ByteArray(vec![1, 2, 3])),
        ("string".into(), Value::String("Hello, World!".into())),
        (
            "list".into(),
            Value::List(vec![
                Value::Compound(BTreeMap::from([(
                    "name".into(),
                    Value::String("Compound 1".into()),
                )])),
                Value::Compound(BTreeMap::from([(
                    "name".into(),
                    Value::String("Compound 2".into()),
                )])),
            ]),
        ),
        (
            "compound".into(),
            Value::Compound(BTreeMap::from([(
                "name".into(),
                Value::String("Compound 3".into()),
            )])),
        ),
    ]));

    let ser = to_net_bytes(&value).unwrap();
    let mut ser_slice = ser.as_slice();
    let ser_le = to_le_bytes(&value).unwrap();
    let mut ser_le_slice = ser_le.as_slice();
    let ser_be = to_be_bytes(&value).unwrap();
    let mut ser_be_slice = ser_be.as_slice();

    from_net_bytes::<Value, _>(&mut ser_slice).unwrap();
    from_le_bytes::<Value, _>(&mut ser_le_slice).unwrap();
    from_be_bytes::<Value, _>(&mut ser_be_slice).unwrap();
}

#[test]
fn read_write_bigtest() {
    #[derive(Deserialize, Serialize, Debug, PartialEq)]
    struct Food {
        name: String,
        value: f32,
    }

    #[derive(Deserialize, Serialize, Debug, PartialEq)]
    struct Nested {
        egg: Food,
        ham: Food,
    }

    #[derive(Deserialize, Serialize, Debug, PartialEq)]
    struct ListCompound {
        #[serde(rename = "created-on")]
        created_on: i64,
        name: String,
    }

    #[derive(Deserialize, Serialize, Debug, PartialEq)]
    struct AllTypes {
        #[serde(rename = "nested compound test")]
        nested: Nested,
        #[serde(rename = "intTest")]
        int_test: i32,
        #[serde(rename = "byteTest")]
        byte_test: i8,
        #[serde(rename = "stringTest")]
        string_test: String,
        #[serde(rename = "listTest (long)")]
        long_list_test: [i64; 5],
        #[serde(rename = "doubleTest")]
        double_test: f64,
        #[serde(rename = "floatTest")]
        float_test: f32,
        #[serde(rename = "longTest")]
        long_test: i64,
        #[serde(rename = "listTest (compound)")]
        compound_list_test: [ListCompound; 2],
        #[serde(
            rename = "byteArrayTest (the first 1000 values of (n*n*255+n*7)%100, starting with n=0 (0, 62, 34, 16, 8, ...))"
        )]
        byte_array_test: Vec<i8>,
        #[serde(rename = "shortTest")]
        short_test: i16,
    }

    let mut big_test_nbt = Cursor::new(BIG_TEST_NBT);
    let decoded: AllTypes = from_be_bytes(&mut big_test_nbt).unwrap();

    let encoded = to_bytes::<BigEndian>(&decoded).unwrap();
    let mut encoded = Cursor::new(encoded.as_slice());
    let _decoded2: AllTypes = from_be_bytes(&mut encoded).unwrap();

    let mut big_test_nbt = Cursor::new(BIG_TEST_NBT);
    let value: Value = from_be_bytes(&mut big_test_nbt).unwrap();

    let value_encoded = to_bytes::<NetworkLittleEndian>(&value).unwrap();
    let mut value_encoded = Cursor::new(value_encoded.as_slice());
    let value_decoded: Value = from_net_bytes(&mut value_encoded).unwrap();
    assert_eq!(value, value_decoded);
}

#[test]
fn read_write_hello_world() {
    #[derive(Deserialize, Serialize, Debug, PartialEq)]
    #[serde(rename = "hello world")]
    struct HelloWorld {
        name: Value,
    }

    let mut copy = Cursor::new(HELLO_WORLD_NBT.to_vec());
    println!("{copy:?}");

    // let decoded: HelloWorld = dbg!(from_be_bytes(&mut HELLO_WORLD_NBT)).unwrap();
    let decoded: Result<HelloWorld, Error> = from_be_bytes(&mut copy);
    if let Err(err) = decoded {
        println!("{err:#}");
        panic!("");
    }

    let decoded = decoded.unwrap();

    let encoded = to_be_bytes(&decoded).unwrap();
    assert_eq!(encoded.as_slice(), HELLO_WORLD_NBT);

    let value: Value = from_be_bytes(&mut HELLO_WORLD_NBT).unwrap();
    let value_encoded = to_be_bytes(&value).unwrap();
    let value_decoded: Value = from_be_bytes(&mut value_encoded.as_slice()).unwrap();
    assert_eq!(value, value_decoded);
}

#[test]
fn read_write_player() {
    #[derive(Deserialize, Serialize, Debug, PartialEq)]
    #[serde(rename_all = "PascalCase")]
    #[serde(rename = "")]
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

    let decoded: Player = from_be_bytes(&mut PLAYER_NAN_VALUE_NBT).unwrap();
    let encoded = to_be_bytes(&decoded).unwrap();
    let decoded2: Player = from_be_bytes(&mut encoded.as_slice()).unwrap();

    let _value: Value = from_be_bytes(&mut PLAYER_NAN_VALUE_NBT).unwrap();
    let value_encoded = to_be_bytes(&decoded2).unwrap();
    let _value_decoded: Value = from_be_bytes(&mut value_encoded.as_slice()).unwrap();
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename = "Holder")]
struct RawHolder {
    value: NbtString,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename = "Holder")]
struct StringHolder {
    value: String,
}

/// A `String` tag whose payload is not valid UTF-8 must round-trip losslessly
/// through [`NbtString`] for every endianness, while deserialising the same
/// payload into a [`String`] must error.
#[test]
fn raw_string_invalid_utf8_roundtrip() {
    // 0xff / 0xfe / 0x80 are not valid UTF-8, and an interior NUL is included
    // to make sure nothing treats the string as NUL-terminated.
    let raw: Vec<u8> = vec![0x68, 0x69, 0xff, 0xfe, 0x00, 0x80, 0x21];
    assert!(std::str::from_utf8(&raw).is_err());

    let holder = RawHolder {
        value: NbtString::from(raw.clone()),
    };

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&holder).unwrap();

            // (a) round-trips into the wrapper, byte-for-byte.
            let back: RawHolder = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, holder);
            assert_eq!(back.value.as_bytes(), raw.as_slice());

            // (b) deserialising the same payload into `String` errors.
            let as_string: Result<StringHolder, Error> = $from(&mut bytes.as_slice());
            assert!(
                as_string.is_err(),
                "expected a UTF-8 error when reading invalid bytes into String, got {as_string:?}"
            );
        }};
    }

    check!(to_be_bytes, from_be_bytes);
    check!(to_le_bytes, from_le_bytes);
    check!(to_net_bytes, from_net_bytes);
}

/// [`NbtString`] must work as a compound (map) key, including keys that are
/// not valid UTF-8.
#[test]
fn raw_string_as_map_key() {
    let invalid_key = NbtString::from(vec![0x6b, 0xff, 0xfe, 0x79]);
    assert!(std::str::from_utf8(invalid_key.as_bytes()).is_err());

    let map: HashMap<NbtString, String> = HashMap::from([
        (NbtString::from("plain"), "value".to_owned()),
        (invalid_key, "raw".to_owned()),
    ]);

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&map).unwrap();
            let back: HashMap<NbtString, String> = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, map);
        }};
    }

    check!(to_be_bytes, from_be_bytes);
    check!(to_le_bytes, from_le_bytes);
    check!(to_net_bytes, from_net_bytes);
}

/// A `Vec<NbtString>` field must round-trip as an NBT List of String tags,
/// including non-UTF-8 elements.
#[test]
fn raw_string_in_list() {
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct ListHolder {
        list: Vec<NbtString>,
    }

    let holder = ListHolder {
        list: vec![
            NbtString::from("first"),
            NbtString::from(vec![0xff, 0xfe, 0x00]),
            NbtString::from("third"),
        ],
    };

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&holder).unwrap();
            let back: ListHolder = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, holder);
        }};
    }

    check!(to_be_bytes, from_be_bytes);
    check!(to_le_bytes, from_le_bytes);
    check!(to_net_bytes, from_net_bytes);
}

/// An empty [`NbtString`] must round-trip and encode identically to an empty
/// [`String`].
#[test]
fn raw_string_empty() {
    let holder = RawHolder {
        value: NbtString::new(Vec::new()),
    };
    let string_holder = StringHolder {
        value: String::new(),
    };

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&holder).unwrap();
            assert_eq!(bytes, $to(&string_holder).unwrap());

            let back: RawHolder = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, holder);
            assert!(back.value.as_bytes().is_empty());
        }};
    }

    check!(to_be_bytes, from_be_bytes);
    check!(to_le_bytes, from_le_bytes);
    check!(to_net_bytes, from_net_bytes);
}

/// A [`Value`] containing non-UTF-8 string values *and* non-UTF-8 compound keys
/// must round-trip losslessly (byte-for-byte after re-serialization) for every
/// endianness.
#[test]
fn value_non_utf8_roundtrip() {
    // 0xff / 0xfe / 0x80 are not valid UTF-8, and an interior NUL is included.
    let raw_value: BString = BString::from(vec![0x68, 0x69, 0xff, 0xfe, 0x00, 0x80, 0x21]);
    let raw_key: BString = BString::from(vec![0x6b, 0xff, 0xfe, 0x79]);
    assert!(std::str::from_utf8(raw_value.as_ref()).is_err());
    assert!(std::str::from_utf8(raw_key.as_ref()).is_err());

    let value = Value::Compound(BTreeMap::from([
        ("plain".into(), Value::String("Hello, World!".into())),
        (raw_key.clone(), Value::String(raw_value.clone())),
        (
            "nested".into(),
            Value::Compound(BTreeMap::from([(
                raw_key.clone(),
                Value::List(vec![
                    Value::String(raw_value.clone()),
                    Value::String("mixed".into()),
                ]),
            )])),
        ),
    ]));

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&value).unwrap();

            // Deserialises back into an equal `Value`...
            let back: Value = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, value);

            // ...and re-serialises to the exact same bytes (lossless).
            let re_encoded = $to(&back).unwrap();
            assert_eq!(re_encoded, bytes);

            // The raw bytes survived untouched.
            let compound = back.as_compound().unwrap();
            assert_eq!(
                compound.get(&raw_key).unwrap().as_string().unwrap(),
                &raw_value
            );
        }};
    }

    check!(to_be_bytes, from_be_bytes);
    check!(to_le_bytes, from_le_bytes);
    check!(to_net_bytes, from_net_bytes);
}

/// A [`Value`] containing a `ByteArray` (including empty and 0xFF-style bytes)
/// alongside a `String` and a `List` must round-trip losslessly and re-encode
/// byte-for-byte for every endianness, with no cross-contamination between the
/// three byte-shaped tags.
#[test]
fn value_byte_array_roundtrip() {
    // Values around 0x00, 0x7f/0x80 and 0xff exercise the boundaries of the
    // signed-byte range.
    let bytes: Vec<u8> = vec![0x00, 0x01, 0x7f, 0x80, 0xfe, 0xff];

    let value = Value::Compound(BTreeMap::from([
        ("empty".into(), Value::ByteArray(Vec::new())),
        ("bytes".into(), Value::ByteArray(bytes.clone())),
        ("string".into(), Value::String("Hello, World!".into())),
        (
            "list".into(),
            Value::List(vec![Value::Byte(1), Value::Byte(2), Value::Byte(3)]),
        ),
    ]));

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let encoded = $to(&value).unwrap();

            let back: Value = $from(&mut encoded.as_slice()).unwrap();
            assert_eq!(back, value);

            // Byte-identical re-encode.
            let re_encoded = $to(&back).unwrap();
            assert_eq!(re_encoded, encoded);

            let compound = back.as_compound().unwrap();

            // The byte array survived as a `ByteArray`, not a `List` or a `String`.
            let decoded_bytes = compound.get(&BString::from("bytes")).unwrap();
            assert!(decoded_bytes.is_byte_array());
            assert_eq!(
                decoded_bytes.as_byte_array().unwrap().as_slice(),
                &bytes[..]
            );

            assert!(
                compound
                    .get(&BString::from("empty"))
                    .unwrap()
                    .is_byte_array()
            );
            assert!(
                compound
                    .get(&BString::from("empty"))
                    .unwrap()
                    .as_byte_array()
                    .unwrap()
                    .is_empty()
            );

            // The string is still a `String`, and the list is still a `List`.
            assert!(compound.get(&BString::from("string")).unwrap().is_string());
            assert!(compound.get(&BString::from("list")).unwrap().is_list());
        }};
    }

    check!(to_be_bytes, from_be_bytes);
    check!(to_le_bytes, from_le_bytes);
    check!(to_net_bytes, from_net_bytes);
}

/// A user struct with an [`NbtByteArray`] field must round-trip as a real NBT
/// `ByteArray` tag for every endianness, and decode into a `Value::ByteArray`.
#[test]
fn nbt_byte_array_field() {
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    #[serde(rename = "Holder")]
    struct Holder {
        data: NbtByteArray,
    }

    let holder = Holder {
        data: NbtByteArray::from(vec![0x00, 0x7f, 0x80, 0xff]),
    };

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&holder).unwrap();

            // Round-trips through the wrapper.
            let back: Holder = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, holder);

            // Decodes into a `Value::ByteArray` (i.e. it really is a ByteArray tag).
            let value: Value = $from(&mut bytes.as_slice()).unwrap();
            let field = value
                .as_compound()
                .unwrap()
                .get(&BString::from("data"))
                .unwrap();
            assert!(field.is_byte_array());
            assert_eq!(
                field.as_byte_array().unwrap().as_slice(),
                holder.data.as_bytes()
            );
        }};
    }

    check!(to_be_bytes, from_be_bytes);
    check!(to_le_bytes, from_le_bytes);
    check!(to_net_bytes, from_net_bytes);
}

/// Valid UTF-8 must still work through both [`String`] and [`NbtString`], and
/// an [`NbtString`] must encode identically to the equivalent [`String`].
#[test]
fn raw_string_valid_utf8() {
    let holder = RawHolder {
        value: NbtString::from("Hello, World!"),
    };
    let string_holder = StringHolder {
        value: "Hello, World!".to_owned(),
    };

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let raw_bytes = $to(&holder).unwrap();
            let string_bytes = $to(&string_holder).unwrap();

            // An NbtString encodes exactly like the equivalent String tag.
            assert_eq!(raw_bytes, string_bytes);

            // Valid UTF-8 still deserialises into a plain String.
            let as_string: StringHolder = $from(&mut raw_bytes.as_slice()).unwrap();
            assert_eq!(as_string, string_holder);

            // ...and round-trips through the wrapper.
            let as_raw: RawHolder = $from(&mut raw_bytes.as_slice()).unwrap();
            assert_eq!(as_raw, holder);
        }};
    }

    check!(to_be_bytes, from_be_bytes);
    check!(to_le_bytes, from_le_bytes);
    check!(to_net_bytes, from_net_bytes);
}
