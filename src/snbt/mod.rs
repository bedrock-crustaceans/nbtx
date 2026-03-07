pub mod de;
pub mod ser;

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde::{Deserialize, Serialize};

    use crate::{
        Value,
        snbt::{de::SnbtDeserializer, ser::Serializer},
    };

    #[derive(Debug, Copy, Clone, serde::Serialize)]
    enum Test {
        A,
        B,
        C,
    }

    #[derive(serde::Serialize)]
    struct Data {
        value: Test,
        tuple: [u8; 5],
    }

    #[test]
    fn simple_snbt() {
        let value = Data {
            value: Test::A,
            tuple: [1; 5],
        };

        let mut ser = Serializer::new();
        value.serialize(&mut ser).unwrap();

        println!("{}", ser.output);
    }

    #[test]
    fn snbt_all() {
        let value = Value::Compound(HashMap::from([
            ("byte".to_owned(), Value::Byte(42)),
            ("short".to_owned(), Value::Short(42)),
            ("int".to_owned(), Value::Int(42)),
            ("long".to_owned(), Value::Long(42)),
            ("float".to_owned(), Value::Float(42.0)),
            ("double 1".to_owned(), Value::Double(42.0)),
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

        let mut ser = Serializer::new();
        value.serialize(&mut ser).unwrap();
        println!("all: {}", ser.output);
    }
}
