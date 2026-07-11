use byteorder::BigEndian;
use nbtx::Value;
use std::collections::BTreeMap;
use std::io::Cursor;

fn main() {
    let value = Value::Compound(BTreeMap::from([(
        "Hello World".into(),
        Value::String("Helloooo World!".into()),
    )]));

    let bytes = nbtx::to_bytes::<BigEndian>(&value).unwrap();

    let res = nbtx::from_bytes::<BigEndian, Value>(&mut Cursor::new(bytes.as_slice())).unwrap();

    assert_eq!(value, res)
}
