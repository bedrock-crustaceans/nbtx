//! The document root name, and [`nbtx::Named`] — the wrapper that keeps it.
//!
//! `cargo run --example named_root`
//!
//! An NBT document is `<tag byte><root name><payload>`. That root name is a
//! genuine part of the file, but almost nothing uses it: every Bedrock document
//! (network packets, `level.dat`, `servers.dat`) leaves it empty, so nbtx's
//! plain entry points always *write* an empty name and *discard* the one they
//! read. Java Edition files, and some server tooling, do carry a name — and
//! dropping it means the file no longer round-trips.

use bstr::BString;
use facet::Facet;
use nbtx::{Named, Value};

/// `tests/fixtures/hello_world.nbt` is the canonical named-root document: a
/// compound named `hello world` holding a single `name` string.
const HELLO_WORLD_NBT: &[u8] = include_bytes!("../tests/fixtures/hello_world.nbt");

#[derive(Facet, Debug, PartialEq)]
struct HelloWorld {
    name: String,
}

fn main() -> Result<(), nbtx::Error> {
    println!("the fixture on disk ({} bytes):", HELLO_WORLD_NBT.len());
    println!("  {}", hex(HELLO_WORLD_NBT));
    println!("  tag 0x0a (compound), name length 0x000b, then \"hello world\"");

    // Decoding straight into the payload type works and is usually what you
    // want, but the root name is gone: re-encoding produces a *different* file.
    // `&[u8]` is a `Read`, and reading advances it, so each decode starts from
    // a fresh copy of the slice.
    let plain: HelloWorld = nbtx::from_be_bytes(&mut &HELLO_WORLD_NBT[..])?;
    let plain_bytes = nbtx::to_be_bytes(&plain)?;
    println!("\nwithout `Named`: {plain:?}");
    println!(
        "  re-encoded to {} bytes, identical to the original: {}",
        plain_bytes.len(),
        plain_bytes == HELLO_WORLD_NBT
    );
    println!("  the 11-byte root name was dropped, so the file changed");

    // `Named<T>` is the opt-in. The codecs recognise it at the root and route
    // the name into its `name` field instead of throwing it away; `T` decodes
    // exactly as it would on its own.
    let named: Named<HelloWorld> = nbtx::from_be_bytes(&mut &HELLO_WORLD_NBT[..])?;
    let named_bytes = nbtx::to_be_bytes(&named)?;
    println!(
        "\nwith `Named`: name = {:?}, value = {:?}",
        named.name, named.value
    );
    println!(
        "  re-encoded to {} bytes, identical to the original: {}",
        named_bytes.len(),
        named_bytes == HELLO_WORLD_NBT
    );
    assert_eq!(named_bytes, HELLO_WORLD_NBT);

    // The same holds without a schema at all: `Named<Value>` is the shape to
    // use for a general-purpose NBT tool that must not perturb the files it
    // touches.
    let dynamic: Named<Value> = nbtx::from_be_bytes(&mut &HELLO_WORLD_NBT[..])?;
    assert_eq!(nbtx::to_be_bytes(&dynamic)?, HELLO_WORLD_NBT);
    println!("\n`Named<Value>` does it schemalessly too: {dynamic:?}");

    // Writing a name is symmetric, and the root does not have to be a compound:
    // nbtx accepts any root tag but `TAG_End`, so a bare scalar is a valid
    // document that reads back as what it was written as.
    let scalar = Named::new("spawn radius", Value::Int(16));
    let encoded = nbtx::to_be_bytes(&scalar)?;
    let back: Named<Value> = nbtx::from_be_bytes(&mut encoded.as_slice())?;
    println!("\na named scalar root: {} -> {back:?}", hex(&encoded));
    assert_eq!(back, scalar);

    // Root names are NBT strings, so like every other NBT string they are raw
    // bytes rather than guaranteed UTF-8 — hence `BString`.
    let odd = Named::new(BString::from(vec![0xff, 0xfe, b'!']), Value::Byte(1));
    let odd_back: Named<Value> = nbtx::from_be_bytes(&mut nbtx::to_be_bytes(&odd)?.as_slice())?;
    println!("a non-UTF-8 root name survives too: {:?}", odd_back.name);
    assert_eq!(odd_back, odd);

    // `Named` is only special at the root. As a struct field it is an ordinary
    // two-key compound, because nested values are already named by their key.
    println!("\n`Named` is meaningful only at the root; nested values are named");
    println!("by their compound key already.");

    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join("")
}
