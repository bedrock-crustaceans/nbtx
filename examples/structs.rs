//! Derived structs in depth: which Rust type produces which NBT tag, how
//! `Option`, nesting, renaming and maps behave, and where the mapping is
//! deliberately lossy.
//!
//! `cargo run --example structs`
//!
//! nbtx has no schema language and no attribute soup: the Rust type *is* the
//! schema. The trade-off is that a few NBT tags share a Rust type, so a derived
//! struct cannot express them all — use `nbtx::Value` (see `examples/value.rs`)
//! when you need one-to-one tag fidelity.

use std::collections::HashMap;

use facet::Facet;
use nbtx::Value;

/// One field per NBT tag reachable from a derived struct.
///
/// The pairs worth memorising: `Vec<u8>` is a `ByteArray` while `bstr::BString`
/// is a `String`, and `Vec<i32>`/`Vec<i64>` become the *typed arrays*
/// `IntArray`/`LongArray` rather than a generic `List`. Any other `Vec<T>`
/// (and any fixed-size `[T; N]` of such a `T`) is a `List`.
#[derive(Facet, Debug, PartialEq)]
struct EveryTag {
    a_bool: bool,
    a_byte: i8,
    a_short: i16,
    an_int: i32,
    a_long: i64,
    a_float: f32,
    a_double: f64,
    a_string: String,
    a_byte_array: Vec<u8>,
    an_int_array: Vec<i32>,
    a_long_array: Vec<i64>,
    a_fixed_int_array: [i32; 3],
    a_list: Vec<String>,
    a_compound: Coordinates,
}

/// A nested struct is just a nested `Compound` tag; nesting has no special
/// syntax and no depth limit short of [`nbtx::MAX_DEPTH`].
#[derive(Facet, Debug, PartialEq)]
struct Coordinates {
    x: f64,
    y: f64,
    z: f64,
}

/// `rename_all` (and per-field `#[facet(rename = "...")]`) changes the compound
/// *key*, which is how a snake_case Rust struct models a camelCase file.
///
/// A container-level `#[facet(rename = "...")]` has no effect on the binary wire
/// format: nbtx always writes an empty root name, so the bytes never depend on
/// a Rust type name. `nbtx::Named` is the way to write a real root name — see
/// `examples/named_root.rs`.
#[derive(Facet, Debug, PartialEq)]
#[facet(rename_all = "camelCase")]
struct ServerEntry {
    server_name: String,
    /// `Option` is the only way to say "this key may be absent". A `None` is
    /// omitted from the output entirely rather than written as a null-ish tag,
    /// and a missing key decodes back to `None`.
    accept_textures: Option<bool>,
}

/// An enum states how its variants are written, and **must**: nbtx has no
/// default for it, because the wire form of an enum is part of a document's
/// schema and a default would let a Rust-side edit change it silently.
///
/// `variant_as(str)` is the readable form — the variant's name as a `String`
/// tag — and honours `#[facet(rename = "...")]` on a variant.
#[derive(Facet, Debug, PartialEq)]
#[facet(nbtx::variant_as(str))]
#[repr(u8)]
enum GameMode {
    Survival,
    #[facet(rename = "creative")]
    Creative,
}

/// The compact form: the variant's *discriminant* as a fixed-width integer tag.
/// `u8`/`i8` → `Byte`, `u16`/`i16` → `Short`, `u32`/`i32` → `Int`, `u64`/`i64` →
/// `Long`. Give the variants explicit numbers to pin the values a document uses;
/// the wire width is independent of the `#[repr(...)]` Rust needs to store them.
#[derive(Facet, Debug, PartialEq)]
#[facet(nbtx::variant_as(u8))]
#[repr(u8)]
enum Difficulty {
    Peaceful = 0,
    Easy = 1,
    Normal = 2,
    Hard = 3,
}

#[derive(Facet, Debug, PartialEq)]
struct WorldSettings {
    game_mode: GameMode,
    difficulty: Difficulty,
}

fn main() -> Result<(), nbtx::Error> {
    let every = EveryTag {
        a_bool: true,
        a_byte: -3,
        a_short: 300,
        an_int: 70_000,
        a_long: 5_000_000_000,
        a_float: 1.5,
        a_double: 2.25,
        a_string: "hello".to_owned(),
        a_byte_array: vec![0xde, 0xad],
        an_int_array: vec![1, 2, 3],
        a_long_array: vec![-1, -2],
        a_fixed_int_array: [7, 8, 9],
        a_list: vec!["first".to_owned(), "second".to_owned()],
        a_compound: Coordinates {
            x: 0.5,
            y: 64.0,
            z: -0.5,
        },
    };

    let bytes = nbtx::to_be_bytes(&every)?;

    // Decoding the same bytes a second time into a `Value` is the easiest way
    // to see what the derive actually emitted: `Value` keeps the exact tag of
    // every node, so it doubles as an inspection tool for your own schemas.
    let inspected: Value = nbtx::from_be_bytes(&mut bytes.as_slice())?;
    println!("what the derive produced, field by field:");
    for (key, value) in inspected.as_compound().expect("a struct is a compound") {
        println!(
            "  {key:<18} tag {:>2}  {}",
            value.discriminant(),
            tag_name(value)
        );
    }

    let back: EveryTag = nbtx::from_be_bytes(&mut bytes.as_slice())?;
    assert_eq!(back, every);
    println!("\n{} bytes, round-trips exactly", bytes.len());

    println!("\nfields declared `Option` are omitted when `None`:");
    for entry in [
        ServerEntry {
            server_name: "Hypixel".to_owned(),
            accept_textures: Some(true),
        },
        ServerEntry {
            server_name: "LAN".to_owned(),
            accept_textures: None,
        },
    ] {
        let encoded = nbtx::to_be_bytes(&entry)?;
        let keys: Value = nbtx::from_be_bytes(&mut encoded.as_slice())?;
        let keys: Vec<_> = keys
            .as_compound()
            .expect("a struct is a compound")
            .keys()
            .map(ToString::to_string)
            .collect();
        // Note the camelCase keys: `rename_all` rewrote them on the way out.
        println!(
            "  {:?} -> {} bytes, keys {keys:?}",
            entry.accept_textures,
            encoded.len()
        );
        let decoded: ServerEntry = nbtx::from_be_bytes(&mut encoded.as_slice())?;
        assert_eq!(decoded, entry);
    }

    // A `HashMap`/`BTreeMap` field is also a compound, but with dynamic keys
    // instead of fixed ones — the right choice when the key set is data, not
    // schema (Bedrock block states, for instance). Mixing a map of `Value`s
    // into an otherwise statically-typed struct is a common and useful shape.
    #[derive(Facet, Debug)]
    struct Block {
        name: String,
        states: HashMap<String, Value>,
    }

    let block = Block {
        name: "minecraft:grass".to_owned(),
        states: HashMap::from([
            ("snowy".to_owned(), Value::Byte(0)),
            ("height".to_owned(), Value::Int(3)),
        ]),
    };
    let encoded = nbtx::to_be_bytes(&block)?;
    let decoded: Block = nbtx::from_be_bytes(&mut encoded.as_slice())?;
    println!("\ndynamic keys via a map field: {decoded:?}");

    // An enum field: one written as text, one as a number, in the same struct.
    let settings = WorldSettings {
        game_mode: GameMode::Creative,
        difficulty: Difficulty::Hard,
    };
    let encoded = nbtx::to_be_bytes(&settings)?;
    let inspected: Value = nbtx::from_be_bytes(&mut encoded.as_slice())?;
    println!("\nenum fields, in the form each one declared:");
    for (key, value) in inspected.as_compound().expect("a struct is a compound") {
        println!(
            "  {key:<12} tag {:>2}  {:<9} {value:?}",
            value.discriminant(),
            tag_name(value)
        );
    }
    let decoded: WorldSettings = nbtx::from_be_bytes(&mut encoded.as_slice())?;
    assert_eq!(decoded, settings);

    println!("\nwhat a derived struct cannot express:");
    // Both of these fields are `Vec<i32>` in Rust, so both come back as an
    // `IntArray`. The distinction between a `List` of `Int`s (tag 9) and an
    // `IntArray` (tag 11) survives only through `Value`.
    println!("  * `List` of `Int` vs `IntArray` — both are `Vec<i32>`");
    println!("  * enums carrying data — unsupported; unit variants do round-trip,");
    println!("    in whichever form `#[facet(nbtx::variant_as(...))]` declares");
    println!("  * a `String` field rejects non-UTF-8 bytes — use `bstr::BString`");
    println!("    (see `examples/non_utf8.rs`)");

    Ok(())
}

/// The tag name for a decoded node. `Value`'s discriminant is the on-wire tag
/// byte, so this doubles as the tag-number legend.
fn tag_name(value: &Value) -> &'static str {
    match value {
        Value::Byte(_) => "Byte",
        Value::Short(_) => "Short",
        Value::Int(_) => "Int",
        Value::Long(_) => "Long",
        Value::Float(_) => "Float",
        Value::Double(_) => "Double",
        Value::ByteArray(_) => "ByteArray",
        Value::String(_) => "String",
        Value::List(_) => "List",
        Value::Compound(_) => "Compound",
        Value::IntArray(_) => "IntArray",
        Value::LongArray(_) => "LongArray",
    }
}
