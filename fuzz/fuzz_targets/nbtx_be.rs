#![no_main]

use std::io::Cursor;
use libfuzzer_sys::fuzz_target;
use nbtx::BigEndian;

fuzz_target!(|data: &[u8]| {
    let mut reader = Cursor::new(data);

    let val: nbtx::Value = nbtx::from_bytes::<BigEndian, _>(&mut reader).unwrap();
});
