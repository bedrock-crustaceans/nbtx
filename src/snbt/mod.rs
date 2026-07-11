//! Implements stringified NBT support. This is the human-readable NBT format that is often
//! used in Minecraft commands.

mod de;
mod ser;

pub use de::{Deserializer, from_string};
pub use ser::{Serializer, to_string};

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use crate::{
        Value,
        snbt::{de::Deserializer, ser::Serializer},
    };

    const WHITESPACED_ALL: &str = r#"
        {
            float: 42f,
            "double 1": 42d,
            byte_array: [1b, 2b, 3b],
            compound: {
                name: "Compound 3"
            },
            list: [
                {
                    name: "Compound 1"
                },
                {
                    name: "Compound 2"
                }
            ],
            string: "Hello, World!",
            long: 42l,
            byte: 42b,
            short: 42s,
            int: 42,
        }
    "#;

    const BIG_TEST_NBT: &[u8] = include_bytes!("../../test/bigtest.nbt");

    #[allow(dead_code)]
    #[derive(Debug, Copy, Clone, serde::Serialize, serde::Deserialize)]
    enum Test {
        A,
        B,
        C,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct Data {
        // value: Test,
        byte: i8,
        tuple: Vec<i32>,
    }

    #[test]
    fn bigtest() {
        #[allow(const_item_mutation)]
        let data: Value = crate::from_be_bytes(&mut BIG_TEST_NBT).unwrap();

        let mut snbt = Serializer::new();
        data.serialize(&mut snbt).unwrap();

        let mut snbt_de = Deserializer::new(&snbt.output);
        let out: Value = Value::deserialize(&mut snbt_de).unwrap();

        println!("{data:#?}");
        println!("{}", snbt.output);

        assert_eq!(data, out);
    }

    #[test]
    fn simple_snbt() {
        let value = Data {
            // value: Test::A,
            byte: 7,
            tuple: vec![1; 5],
        };

        let mut ser = Serializer::new();
        value.serialize(&mut ser).unwrap();

        println!("{}", ser.output);

        let mut de = Deserializer::new(&ser.output);
        let val = Data::deserialize(&mut de).unwrap();

        println!("Deserialised: {val:?}");
    }

    /// A bare [`crate::NbtString`] must render as a quoted SNBT string (via the
    /// raw-string token), not as a `[B;...]` byte array, and a
    /// [`crate::NbtByteArray`] must render as a `[B;...]` literal.
    #[test]
    fn snbt_raw_string_and_byte_array() {
        #[derive(Debug, serde::Serialize)]
        struct Holder {
            name: crate::NbtString,
            data: crate::NbtByteArray,
        }

        let holder = Holder {
            name: crate::NbtString::from("Hello, World!"),
            data: crate::NbtByteArray::from(vec![1, 2, 0xff]),
        };

        let out = crate::snbt::to_string(&holder).unwrap();
        assert_eq!(out, r#"{name:"Hello, World!",data:[B;1b,2b,-1b]}"#);

        // The byte array round-trips through a `Value`.
        let value: Value = crate::snbt::from_string(&out).unwrap();
        let compound = value.as_compound().unwrap();
        assert_eq!(
            compound.get(&crate::BString::from("name")).unwrap(),
            &Value::String("Hello, World!".into())
        );
        assert_eq!(
            compound.get(&crate::BString::from("data")).unwrap(),
            &Value::ByteArray(vec![1, 2, 0xff])
        );
    }

    #[test]
    fn snbt_all() {
        // let value = Value::Compound(HashMap::from([
        //     ("byte".to_owned(), Value::Byte(42)),
        //     ("short".to_owned(), Value::Short(42)),
        //     ("int".to_owned(), Value::Int(42)),
        //     ("long".to_owned(), Value::Long(42)),
        //     ("float".to_owned(), Value::Float(42.0)),
        //     ("double 1".to_owned(), Value::Double(42.0)),
        //     ("byte_array".to_owned(), Value::ByteArray(vec![1, 2, 3])),
        //     (
        //         "string".to_owned(),
        //         Value::String("Hello, World!".to_owned()),
        //     ),
        //     (
        //         "list".to_owned(),
        //         Value::List(vec![
        //             Value::Compound(HashMap::from([(
        //                 "name".to_owned(),
        //                 Value::String("Compound 1".to_owned()),
        //             )])),
        //             Value::Compound(HashMap::from([(
        //                 "name".to_owned(),
        //                 Value::String("Compound 2".to_owned()),
        //             )])),
        //         ]),
        //     ),
        //     (
        //         "compound".to_owned(),
        //         Value::Compound(HashMap::from([(
        //             "name".to_owned(),
        //             Value::String("Compound 3".to_owned()),
        //         )])),
        //     ),
        // ]));

        // let mut ser = Serializer::new();
        // value.serialize(&mut ser).unwrap();

        // let output = ser.output.clone();
        // println!("output: {output}");

        // let mut de = Deserializer::new(&output);
        // let out: Value = Value::deserialize(&mut de).unwrap();

        let mut de = Deserializer::new(WHITESPACED_ALL);
        let out_newline = Value::deserialize(&mut de).unwrap();

        println!("out_newline: {out_newline:?}");
    }
}
