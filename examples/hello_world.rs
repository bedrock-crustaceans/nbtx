use std::io::Cursor;
use byteorder::BigEndian;
use nbtx::Value;

fn main() {
    let value = Value::String("Hello World!".to_string());

    let bytes = nbtx::to_bytes::<BigEndian>(&value).unwrap();

    let res = nbtx::from_le_bytes(&mut Cursor::new(&*bytes)).unwrap();

    assert_eq!(value, res)
}