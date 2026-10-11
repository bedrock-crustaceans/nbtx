//! Java's modified UTF-8, used for strings in big-endian (Java Edition) NBT.
//!
//! It differs from UTF-8 in two ways: `U+0000` is written as `C0 80`, and a
//! character above `U+FFFF` is written as a UTF-16 surrogate pair, each half
//! encoded in three bytes.

use std::borrow::Cow;
use std::string::FromUtf8Error;

pub(crate) fn encode(s: &str) -> Cow<'_, [u8]> {
    if !s.chars().any(|c| c == '\0' || c > '\u{FFFF}') {
        return Cow::Borrowed(s.as_bytes());
    }
    let mut out = Vec::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '\0' => out.extend([0xC0, 0x80]),
            c if c > '\u{FFFF}' => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    push_three_bytes(&mut out, *unit);
                }
            }
            c => out.extend(c.encode_utf8(&mut [0; 4]).as_bytes()),
        }
    }
    Cow::Owned(out)
}

fn push_three_bytes(out: &mut Vec<u8>, unit: u16) {
    out.extend([
        0xE0 | (unit >> 12) as u8,
        0x80 | ((unit >> 6) & 0x3F) as u8,
        0x80 | (unit & 0x3F) as u8,
    ]);
}

/// Decodes modified UTF-8. Plain UTF-8 (including four-byte sequences) is
/// accepted as well; anything else returns the plain UTF-8 error.
pub(crate) fn decode(bytes: Vec<u8>) -> Result<String, FromUtf8Error> {
    let error = match String::from_utf8(bytes) {
        Ok(s) => return Ok(s),
        Err(error) => error,
    };
    match decode_modified(error.as_bytes()) {
        Some(s) => Ok(s),
        None => Err(error),
    }
}

fn decode_modified(bytes: &[u8]) -> Option<String> {
    let mut units: Vec<u16> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < 0x80 {
            units.push(b as u16);
            i += 1;
        } else if b & 0xE0 == 0xC0 {
            let b1 = continuation(bytes, i + 1)?;
            units.push(((b as u16 & 0x1F) << 6) | b1);
            i += 2;
        } else if b & 0xF0 == 0xE0 {
            let b1 = continuation(bytes, i + 1)?;
            let b2 = continuation(bytes, i + 2)?;
            units.push(((b as u16 & 0x0F) << 12) | (b1 << 6) | b2);
            i += 3;
        } else if b & 0xF8 == 0xF0 {
            let c = std::str::from_utf8(bytes.get(i..i + 4)?)
                .ok()?
                .chars()
                .next()?;
            let mut pair = [0u16; 2];
            units.extend_from_slice(c.encode_utf16(&mut pair));
            i += 4;
        } else {
            return None;
        }
    }
    String::from_utf16(&units).ok()
}

fn continuation(bytes: &[u8], i: usize) -> Option<u16> {
    let b = *bytes.get(i)?;
    (b & 0xC0 == 0x80).then_some((b & 0x3F) as u16)
}
