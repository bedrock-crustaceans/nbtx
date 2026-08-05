//! NBT strings are raw bytes, not UTF-8 — and how to keep them that way.
//!
//! `cargo run --example non_utf8`
//!
//! The NBT specification calls tag 8 a "string", but neither Bedrock nor the
//! files in the wild guarantee it is valid UTF-8: legacy worlds, hand-edited
//! server lists and mod data all contain byte sequences Rust's `String` refuses
//! to hold. A decoder that assumes UTF-8 either errors on those files or, worse,
//! replaces the offending bytes and silently corrupts them on the way back out.
//!
//! nbtx's answer is [`bstr::BString`]: a `Vec<u8>` that prints like a string.
//! Use it wherever a payload might not be text.

use bstr::{BStr, BString};
use facet::Facet;
use nbtx::{Compound, Value};

/// The same NBT document decoded two ways. Both structs have one `String`-tag
/// field; only the `BString` one can hold arbitrary bytes.
#[derive(Facet, Debug, PartialEq)]
struct Lossless {
    label: BString,
}

#[derive(Facet, Debug, PartialEq)]
struct Lossy {
    label: String,
}

fn main() -> Result<(), nbtx::Error> {
    // A lone 0xFF is not valid UTF-8 in any position. Minecraft's section sign
    // (0xC2 0xA7 in UTF-8) is often written as the bare byte 0xA7 by older
    // tools, which produces exactly this situation.
    let raw = BString::from(vec![0xff, 0xfe, b'h', b'i', 0xa7, b'6']);

    let original = Lossless { label: raw.clone() };
    let bytes = nbtx::to_be_bytes(&original)?;
    println!("wrote a `BString` field of {} raw bytes:", raw.len());
    println!("  {raw:?}");
    println!("  as NBT: {}", hex(&bytes));

    // Round-tripping through `BString` returns the same bytes, untouched.
    let back: Lossless = nbtx::from_be_bytes(&mut bytes.as_slice())?;
    assert_eq!(back, original);
    println!(
        "  read back through `BString`: {:?} (identical)",
        back.label
    );

    // The same bytes into a `String` field fail — deliberately. nbtx will not
    // guess a replacement, because a lossy decode that then re-encodes is how
    // files get quietly damaged.
    match nbtx::from_be_bytes::<Lossy>(&mut bytes.as_slice()) {
        Ok(value) => unreachable!("`String` must not accept invalid UTF-8: {value:?}"),
        Err(err) => println!("  read back through `String`: {err}"),
    }

    // A `BString` field is a `String` tag (8), *not* a `ByteArray` (7). That
    // distinction matters for interoperability: a reader expecting a name will
    // look for tag 8. If you actually want a byte array, use `Vec<u8>`.
    let inspect: Value = nbtx::from_be_bytes(&mut bytes.as_slice())?;
    let label = &inspect.as_compound().expect("a compound")[BStr::new("label")];
    println!(
        "  the tag written was {} (String), not 7 (ByteArray)",
        label.discriminant()
    );

    // `Value::String` is a `BString` for the same reason, so the schemaless
    // path is lossless without any extra care.
    println!("\nthe dynamic path needs no opt-in:");
    let dynamic = Value::Compound(Compound::from_iter([
        ("label".into(), Value::String(raw.clone())),
        // Compound *keys* are `BString` too — a key is just another NBT string,
        // and files with non-UTF-8 keys exist.
        (BString::from(vec![0xff, b'k', b'e', b'y']), Value::Byte(1)),
    ]));
    let encoded = nbtx::to_be_bytes(&dynamic)?;
    let decoded: Value = nbtx::from_be_bytes(&mut encoded.as_slice())?;
    assert_eq!(decoded, dynamic);
    for (key, value) in decoded.as_compound().expect("a compound") {
        println!("  key {key:?} -> {value:?}");
    }
    println!("  both the value and the non-UTF-8 key survived");

    // When the bytes *are* text, `BString` still behaves like one: it derefs to
    // `[u8]` and bstr gives it lossy conversions for display.
    println!("\n`BString` is ergonomic for the ordinary case as well:");
    let text = BString::from("plain ascii");
    println!("  from a &str literal: {text:?}, {} bytes", text.len());
    // Only convert when you are about to *display* the bytes, never before
    // re-encoding them — the replacement characters are not reversible.
    println!(
        "  lossy view for display only: {:?}",
        String::from_utf8_lossy(&raw)
    );

    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join("")
}
