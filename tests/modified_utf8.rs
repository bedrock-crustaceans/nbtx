#![cfg(feature = "nbt")]

use std::collections::BTreeMap;

use nbtx::{Value, from_be_bytes, from_le_bytes, to_be_bytes, to_le_bytes};

const TEXT: &str = "\0\u{1F600}";
const JAVA_WRITE_UTF: [u8; 10] = [0x00, 0x08, 0xC0, 0x80, 0xED, 0xA0, 0xBD, 0xED, 0xB8, 0x80];

fn string_root(payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![8, 0, 0];
    bytes.extend(payload);
    bytes
}

#[test]
fn big_endian_strings_are_written_as_java_modified_utf8() {
    assert_eq!(
        to_be_bytes(&TEXT.to_string()).unwrap(),
        string_root(&JAVA_WRITE_UTF)
    );
}

#[test]
fn big_endian_strings_read_java_modified_utf8() {
    let text: String = from_be_bytes(&mut string_root(&JAVA_WRITE_UTF).as_slice()).unwrap();
    assert_eq!(text, TEXT);
}

#[test]
fn big_endian_map_keys_use_java_modified_utf8() {
    let map = BTreeMap::from([(TEXT.to_string(), 1i8)]);
    let bytes = to_be_bytes(&map).unwrap();
    assert_eq!(&bytes[3..6], &[1, 0x00, 0x08]);
    assert_eq!(&bytes[6..14], &JAVA_WRITE_UTF[2..]);
    let back: BTreeMap<String, i8> = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(back, map);
}

#[test]
fn big_endian_strings_still_read_plain_utf8() {
    let mut payload = vec![0x00, 0x05, 0x00];
    payload.extend(&TEXT.as_bytes()[1..]);
    let text: String = from_be_bytes(&mut string_root(&payload).as_slice()).unwrap();
    assert_eq!(text, TEXT);
}

#[test]
fn little_endian_strings_stay_plain_utf8() {
    let bytes = to_le_bytes(&TEXT.to_string()).unwrap();
    assert_eq!(&bytes[3..5], &[5, 0]);
    assert_eq!(&bytes[5..], TEXT.as_bytes());
    let text: String = from_le_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(text, TEXT);
}

#[test]
fn big_endian_values_keep_raw_bytes() {
    let value: Value = from_be_bytes(&mut string_root(&JAVA_WRITE_UTF).as_slice()).unwrap();
    assert_eq!(to_be_bytes(&value).unwrap(), string_root(&JAVA_WRITE_UTF));
}
