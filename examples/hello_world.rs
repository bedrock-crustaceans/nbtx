use byteorder::BigEndian;
use nbtx::Value;
use std::collections::HashMap;
use std::io::Cursor;

fn main() {
    let value = Value::Compound(HashMap::from([(
        "Hello World".to_string(),
        Value::String("Helloooo World!".to_string()),
    )]));

    let bytes = nbtx::to_bytes::<BigEndian>(&value).unwrap();

    let res = nbtx::from_bytes::<BigEndian, Value>(&mut Cursor::new(bytes.as_slice())).unwrap();

    assert_eq!(value, res)
}
