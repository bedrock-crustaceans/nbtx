#![no_main]

use std::io::Cursor;
use libfuzzer_sys::fuzz_target;
use nbtx::LittleEndian;

fuzz_target!(|data: &[u8]| {
    let mut reader = Cursor::new(data);

    let _: nbtx::Value = nbtx::from_bytes::<LittleEndian, _>(&mut reader).unwrap();
});
