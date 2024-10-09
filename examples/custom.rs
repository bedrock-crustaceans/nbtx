use std::collections::HashMap;
use std::io::Cursor;

mod block_version {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Option<[u8; 4]>, D::Error> {
        // Deserialize an NBT `Int` if there is one.
        let word = Option::<i32>::deserialize(de)?;
        // Map the int to 4 bytes (major, minor, patch, ...) to get the block version
        Ok(word.map(i32::to_be_bytes))
    }

    pub fn serialize<S: Serializer>(v: &Option<[u8; 4]>, ser: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(b) => ser.serialize_i32(i32::from_be_bytes(*b)),
            _ => ser.serialize_none()
        }
    }
}

#[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize)]
struct Block {
    pub name: String,
    #[serde(with = "block_version")]
    pub version: Option<[u8; 4]>,
    pub states: HashMap<String, nbtx::Value>
}

fn main() {
    let block = Block {
        name: "minecraft:grass".to_owned(),
        version: Some([1, 20, 1, 0]),
        states: HashMap::from([
            ("bool_field".into(), nbtx::Value::Byte(1)),
            ("list_field".into(), nbtx::Value::List(vec![
                nbtx::Value::Float(42.0), nbtx::Value::Float(45.0)
            ]))
        ])
    };

    let bytes = nbtx::to_be_bytes(&block).unwrap();
    println!("{bytes:?}");

    let mut cursor = Cursor::new(bytes.clone());
    let typed: Block = nbtx::from_be_bytes(&mut cursor).unwrap();

    println!("Typed (Block): {typed:?}");

    let mut cursor = Cursor::new(bytes);
    let untyped: nbtx::Value = nbtx::from_be_bytes(&mut cursor).unwrap();

    println!("Untyped (nbtx::Value): {untyped:?}");
}
