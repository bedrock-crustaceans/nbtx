//! Reading a real file: Minecraft's `servers.dat`.
//!
//! `cargo run --example server_dat`
//!
//! `servers.dat` is the multiplayer server list, stored as uncompressed
//! big-endian NBT at the root of the game directory (`.minecraft/servers.dat`).
//! It is a good specimen because it is small, real, and exercises the awkward
//! parts: a `List` of `Compound`s, an optional key, a boolean stored as a Byte,
//! and a base64 server icon long enough to make truncation obvious.
//!
//! The copy here is checked in next to this file, so the example runs with no
//! Minecraft install.

use byteorder::BigEndian;
use facet::Facet;
use nbtx::Value;

const SERVERS_DAT: &[u8] = include_bytes!("servers.dat");

#[derive(Facet, Debug)]
struct ServerList {
    servers: Vec<ServerEntry>,
}

/// The on-disk keys are camelCase while Rust wants snake_case, which is exactly
/// what `rename_all` is for.
///
/// Every key the file actually contains is modelled here, so no escape hatch is
/// needed: unknown keys are rejected by default. If you only want a subset, say
/// so explicitly with `#[facet(nbtx::allow_unknown_fields)]` — see
/// `examples/unknown_fields.rs`.
#[derive(Facet, Debug)]
#[facet(rename_all = "camelCase")]
struct ServerEntry {
    name: String,
    ip: String,
    /// A base64 PNG. Present for most entries, absent for hand-added ones,
    /// hence `Option`.
    icon: Option<String>,
    /// NBT has no boolean tag; the game writes 0 or 1 as a Byte and nbtx maps a
    /// Rust `bool` onto it. Only `0x01` decodes as `true`, matching Bedrock's
    /// reference decoder.
    hidden: bool,
    /// Written by some launchers, absent from this file. A missing key decodes
    /// as `None` rather than failing.
    accept_textures: Option<bool>,
}

fn main() -> Result<(), nbtx::Error> {
    println!("{} bytes of big-endian NBT\n", SERVERS_DAT.len());

    let list: ServerList = nbtx::from_bytes::<BigEndian, _>(&mut &SERVERS_DAT[..])?;
    for entry in &list.servers {
        println!("{}", entry.name);
        println!("  address:  {}", entry.ip);
        println!("  hidden:   {}", entry.hidden);
        println!(
            "  icon:     {}",
            match &entry.icon {
                Some(icon) => format!("{} base64 characters", icon.len()),
                None => "none".to_owned(),
            }
        );
        println!("  textures: {:?}", entry.accept_textures);
    }

    // The same file with no schema at all. This is what a general-purpose tool
    // (an editor, a diff, a converter) would do — and because `Value` preserves
    // every tag and, under the default `preserve_order` feature, the on-disk key
    // order, re-encoding reproduces the original file exactly. That byte
    // identity is the strongest evidence that nothing was lost in between.
    let dynamic: Value = nbtx::from_bytes::<BigEndian, _>(&mut &SERVERS_DAT[..])?;
    let reencoded = nbtx::to_bytes::<BigEndian>(&dynamic)?;
    println!("\nschemaless view:");
    describe(&dynamic, 1);
    println!(
        "\nre-encoded to {} bytes, byte-identical to the original: {}",
        reencoded.len(),
        reencoded == SERVERS_DAT
    );
    if cfg!(feature = "preserve_order") {
        assert_eq!(reencoded, SERVERS_DAT);
    } else {
        // Built with `--no-default-features`, `Compound` is a sorted `BTreeMap`.
        // No tag or payload is lost, but the keys come back alphabetically
        // (`hidden, icon, ip, name`) rather than in the order the game wrote
        // them, so the bytes differ. Byte identity is what `preserve_order`
        // buys.
        println!("  (`preserve_order` is off, so keys were re-sorted alphabetically)");
    }

    Ok(())
}

/// Prints the tag structure of a `Value` without dumping the 15 KB of icon data
/// a plain `{:#?}` would.
fn describe(value: &Value, indent: usize) {
    let pad = "  ".repeat(indent);
    match value {
        Value::Compound(map) => {
            for (key, child) in map {
                match child {
                    Value::Compound(_) | Value::List(_) => {
                        println!("{pad}{key} ({})", summary(child));
                        describe(child, indent + 1);
                    }
                    _ => println!("{pad}{key} ({})", summary(child)),
                }
            }
        }
        Value::List(items) => {
            // Only the first element is expanded: every element of a list shares
            // one tag, so the rest have the same shape by construction.
            if let Some(first) = items.first() {
                println!("{pad}[0] ({})", summary(first));
                describe(first, indent + 1);
                if items.len() > 1 {
                    println!("{pad}[1..{}] same shape", items.len());
                }
            }
        }
        _ => {}
    }
}

fn summary(value: &Value) -> String {
    match value {
        Value::Byte(v) => format!("Byte {v}"),
        Value::Short(v) => format!("Short {v}"),
        Value::Int(v) => format!("Int {v}"),
        Value::Long(v) => format!("Long {v}"),
        Value::Float(v) => format!("Float {v}"),
        Value::Double(v) => format!("Double {v}"),
        Value::ByteArray(v) => format!("ByteArray, {} bytes", v.len()),
        Value::String(v) if v.len() > 32 => format!("String, {} bytes", v.len()),
        Value::String(v) => format!("String {v:?}"),
        Value::List(v) => format!("List, {} entries", v.len()),
        Value::Compound(v) => format!("Compound, {} keys", v.len()),
        Value::IntArray(v) => format!("IntArray, {} entries", v.len()),
        Value::LongArray(v) => format!("LongArray, {} entries", v.len()),
    }
}
