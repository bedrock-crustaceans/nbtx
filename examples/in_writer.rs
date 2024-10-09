use std::io::Write;
use byteorder::{BigEndian, LittleEndian, WriteBytesExt};

pub fn main() {
    let mut writer = Vec::new();

    writer.write_u16::<LittleEndian>(69).unwrap();
    writer.write_u128::<BigEndian>(42).unwrap();
    // You can write a nbt into an existing writer without any problems!
    nbtx::to_bytes_in::<LittleEndian>(&mut writer, &vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]).unwrap();
    writer.write_all("Hellooo nbtx!".as_bytes()).unwrap();
}
