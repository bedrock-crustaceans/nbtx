//! Unknown compound keys are an error by default, and how to opt out.
//!
//! `cargo run --example unknown_fields`
//!
//! When a file contains a key your struct has no field for, a decoder has two
//! choices: drop it or complain. nbtx complains. Schema drift is the normal
//! state of Minecraft data — a game update adds a key, a plugin writes an extra
//! one — and silently dropping keys means a read-modify-write cycle deletes
//! data the user never knew was there.

use facet::Facet;
use nbtx::{Error, Value};

/// The file's real schema, as written by a newer version of the game.
#[derive(Facet, Debug)]
struct PlayerV2 {
    name: String,
    level: i32,
    /// Added in a later version; older readers know nothing about it.
    prestige: i32,
}

/// An out-of-date reader: no `prestige` field.
#[derive(Facet, Debug)]
struct PlayerV1 {
    name: String,
    level: i32,
}

/// The same out-of-date reader, opted into lenient decoding.
///
/// The attribute is a namespaced facet extension attribute, so it needs no
/// separate derive — nbtx reads it back off the shape at runtime. Use it when
/// you genuinely only want a subset of the keys and dropping the rest is
/// intentional, not accidental.
#[derive(Facet, Debug)]
#[facet(nbtx::allow_unknown_fields)]
struct PlayerSubset {
    name: String,
    level: i32,
}

fn main() -> Result<(), nbtx::Error> {
    let bytes = nbtx::to_be_bytes(&PlayerV2 {
        name: "Steve".to_owned(),
        level: 42,
        prestige: 3,
    })?;
    println!("the file on disk has keys: {:?}", keys(&bytes)?);

    // The default. The error names both the offending key and the struct that
    // rejected it, and its `Display` even suggests the escape hatch.
    println!("\ndecoding into a struct without `prestige`:");
    match nbtx::from_be_bytes::<PlayerV1>(&mut bytes.as_slice()) {
        Ok(value) => unreachable!("an unknown key must not be accepted: {value:?}"),
        Err(Error::UnknownField(err)) => {
            println!("  Error::UnknownField");
            println!("    field:     {:?}", err.field());
            println!("    container: {:?}", err.container());
            println!("    message:   {err}");
        }
        Err(other) => unreachable!("unexpected error: {other}"),
    }

    // The opt-out, per struct.
    println!("\nthe same struct with `#[facet(nbtx::allow_unknown_fields)]`:");
    let lenient: PlayerSubset = nbtx::from_be_bytes(&mut bytes.as_slice())?;
    println!("  decoded {lenient:?}");
    println!("  `prestige` was parsed (so the stream stayed in sync) and dropped");

    // Two shapes accept any key without an attribute, because "unknown" is not
    // a meaningful notion for either.
    println!("\nno attribute needed when the key set is data rather than schema:");
    let dynamic: Value = nbtx::from_be_bytes(&mut bytes.as_slice())?;
    println!("  `Value` keeps everything: {dynamic:?}");

    // A map (`Def::Map`) is exempt for the same reason: a map's keys *are* its
    // data, so no key can be "unknown". This holds for a map field inside a
    // struct as well as for a map at the root, as here.
    let map: std::collections::HashMap<String, Value> = nbtx::from_be_bytes(&mut bytes.as_slice())?;
    println!("  a map target accepts any key: {} entries", map.len());

    // Rejection is not the same as being unable to read old files: it points at
    // the exact key so you can decide whether to add the field, widen it to
    // `Option`, or opt out.
    println!("\nthe point is that schema drift surfaces instead of eating data.");

    Ok(())
}

/// The top-level keys of an encoded document, via the schemaless path.
fn keys(bytes: &[u8]) -> Result<Vec<String>, nbtx::Error> {
    let mut reader = bytes;
    let value: Value = nbtx::from_be_bytes(&mut reader)?;
    Ok(value
        .as_compound()
        .expect("a struct encodes to a compound")
        .keys()
        .map(ToString::to_string)
        .collect())
}
