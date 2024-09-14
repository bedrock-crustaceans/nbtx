#![allow(const_item_mutation)] // We make use of constant mutation on purpose in this test.

use std::collections::HashMap;
use std::io::Cursor;

use byteorder::{BigEndian, LittleEndian};
use serde::{Deserialize, Serialize};

use crate::{de::from_bytes, ser::to_bytes, NetworkLittleEndian, Value};

const BIG_TEST_NBT: &[u8] = include_bytes!("../test/bigtest.nbt");
const HELLO_WORLD_NBT: &[u8] = include_bytes!("../test/hello_world.nbt");
const PLAYER_NAN_VALUE_NBT: &[u8] = include_bytes!("../test/player_nan_value.nbt");

#[test]
fn read_write_salad() {
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Salad {
        name: String,
        quantity: i32,
    }

    let salad = Salad {
        name: String::from("Caesar Salad"),
        quantity: 3,
    };

    let some_ser = to_bytes::<BigEndian>(&salad).unwrap();
    let mut some_ser_slice = Cursor::new(some_ser.as_slice());

    let some_de: Value = from_bytes::<BigEndian, Value>(&mut some_ser_slice).unwrap();
    dbg!(some_de);
}

#[test]
fn read_write_all() {
    let value = Value::Compound(HashMap::from([
        ("byte".to_owned(), Value::Byte(42)),
        ("short".to_owned(), Value::Short(42)),
        ("int".to_owned(), Value::Int(42)),
        ("long".to_owned(), Value::Long(42)),
        ("float".to_owned(), Value::Float(42.0)),
        ("double".to_owned(), Value::Double(42.0)),
        ("byte_array".to_owned(), Value::ByteArray(vec![1, 2, 3])),
        (
            "string".to_owned(),
            Value::String("Hello, World!".to_owned()),
        ),
        (
            "list".to_owned(),
            Value::List(vec![
                Value::Compound(HashMap::from([(
                    "name".to_owned(),
                    Value::String("Compound 1".to_owned()),
                )])),
                Value::Compound(HashMap::from([(
                    "name".to_owned(),
                    Value::String("Compound 2".to_owned()),
                )])),
            ]),
        ),
        (
            "compound".to_owned(),
            Value::Compound(HashMap::from([(
                "name".to_owned(),
                Value::String("Compound 3".to_owned()),
            )])),
        ),
    ]));

    let ser_var = to_bytes::<NetworkLittleEndian>(&value).unwrap();
    let mut ser_slice = Cursor::new(ser_var.as_slice());
    let ser_le = to_bytes::<LittleEndian>(&value).unwrap();
    let mut ser_le_slice = Cursor::new(ser_le.as_slice());
    let ser_be = to_bytes::<BigEndian>(&value).unwrap();
    let mut ser_be_slice = Cursor::new(ser_be.as_slice());

    from_bytes::<NetworkLittleEndian, Value>(&mut ser_slice).unwrap();
    from_bytes::<LittleEndian, Value>(&mut ser_le_slice).unwrap();
    from_bytes::<BigEndian, Value>(&mut ser_be_slice).unwrap();
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

    let mut big_test_nbt = Cursor::new(BIG_TEST_NBT.as_ref());
    let decoded: AllTypes = from_bytes::<BigEndian, _>(&mut big_test_nbt).unwrap();

    let encoded = to_bytes::<BigEndian>(&decoded).unwrap();
    let mut encoded = Cursor::new(encoded.as_slice());
    let _decoded2: AllTypes = from_bytes::<BigEndian, _>(&mut encoded).unwrap();

    let mut big_test_nbt = Cursor::new(BIG_TEST_NBT.as_ref());
    let value: Value = from_bytes::<BigEndian, _>(&mut big_test_nbt).unwrap();
    let value_encoded = to_bytes::<BigEndian>(&value).unwrap();
    let mut value_encoded = Cursor::new(value_encoded.as_slice());
    let value_decoded: Value = from_bytes::<BigEndian, _>(&mut value_encoded).unwrap();
    assert_eq!(value, value_decoded);
}

#[test]
fn read_write_hello_world() {
    #[derive(Deserialize, Serialize, Debug, PartialEq)]
    #[serde(rename = "hello world")]
    struct HelloWorld {
        name: Value,
    }

    let mut hello_world_nbt = Cursor::new(HELLO_WORLD_NBT);
    let decoded: HelloWorld = from_bytes::<BigEndian, _>(&mut hello_world_nbt).unwrap();
    let encoded = to_bytes::<BigEndian>(&decoded).unwrap();
    assert_eq!(encoded.as_slice(), HELLO_WORLD_NBT);

    let mut hello_world_nbt = Cursor::new(HELLO_WORLD_NBT);
    let value: Value = from_bytes::<BigEndian, _>(&mut hello_world_nbt).unwrap();
    let value_encoded = to_bytes::<BigEndian>(&value).unwrap();
    let mut value_encoded = Cursor::new(value_encoded.as_slice());
    let value_decoded: Value = from_bytes::<BigEndian, _>(&mut value_encoded).unwrap();
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

    let mut player_nan_value_nbt = Cursor::new(PLAYER_NAN_VALUE_NBT);
    let decoded: Player = from_bytes::<BigEndian, _>(&mut player_nan_value_nbt).unwrap();
    let encoded = to_bytes::<BigEndian>(&decoded).unwrap();
    let mut encoded = Cursor::new(encoded.as_slice());
    let decoded2: Player = from_bytes::<BigEndian, _>(&mut encoded).unwrap();

    let mut player_nan_value_nbt = Cursor::new(PLAYER_NAN_VALUE_NBT);
    let _value: Value = from_bytes::<BigEndian, _>(&mut player_nan_value_nbt).unwrap();
    let value_encoded = to_bytes::<BigEndian>(&decoded2).unwrap();
    let mut value_encoded = Cursor::new(value_encoded.as_slice());
    let _value_decoded: Value = from_bytes::<BigEndian, _>(&mut value_encoded).unwrap();
}
