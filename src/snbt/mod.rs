pub mod de;
pub mod ser;

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde::{Deserialize, Serialize};

    use crate::{
        Value,
        snbt::{de::Deserializer, ser::Serializer},
    };

    const WHITESPACED_ALL: &'static str = r#"
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
    #[derive(Debug, Copy, Clone, serde::Serialize)]
    enum Test {
        A,
        B,
        C,
    }

    #[derive(serde::Serialize)]
    struct Data {
        value: Test,
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
            value: Test::A,
            byte: 7,
            tuple: vec![1; 5],
        };

        let mut ser = Serializer::new();
        value.serialize(&mut ser).unwrap();

        println!("{}", ser.output);

        let mut de = Deserializer::new(&ser.output);
        let val = Value::deserialize(&mut de).unwrap();

        println!("Deserialised: {val:?}");
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
