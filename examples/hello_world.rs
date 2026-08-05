//! The thirty-second introduction: derive [`Facet`] on a plain Rust struct,
//! encode it to NBT bytes, decode it back.
//!
//! `cargo run --example hello_world`
//!
//! Every other example builds on this one. If you only read one, read this.

// `facet` is a direct dependency, pinned to the same git commit nbtx pins (see
// its Cargo.toml). It has to be: `#[derive(Facet)]` expands to `::facet::`
// paths, so the deriving crate needs facet in scope itself — and it must be the
// *same* rev, or the derived impl targets a different trait than nbtx's bounds.
use facet::Facet;

/// A field's Rust type decides its NBT tag: `String` becomes a `String` tag,
/// `i32` an `Int`, `f32` a `Float`. `examples/structs.rs` walks the full table.
#[derive(Facet, Debug, PartialEq)]
struct Player {
    name: String,
    level: i32,
    health: f32,
}

fn main() -> Result<(), nbtx::Error> {
    let player = Player {
        name: "Steve".to_owned(),
        level: 42,
        health: 19.5,
    };

    // Three wire variants exist; `to_be_bytes` is the big-endian one used by
    // Java Edition and by most `.nbt` files found in the wild. Bedrock uses
    // `to_le_bytes` on disk and `to_varint_bytes` on the network — same API,
    // see `examples/endianness.rs`.
    let bytes = nbtx::to_be_bytes(&player)?;
    println!("{} encoded to {} bytes of NBT:", player.name, bytes.len());
    println!("  {}", hex(&bytes));

    // The decoder reads from any `std::io::Read`, so a byte slice works as-is;
    // a `File` or a network socket would too. The target type is inferred from
    // the binding, and must be spelled out somewhere because nothing in the
    // byte stream says which Rust struct it belongs to.
    let decoded: Player = nbtx::from_be_bytes(&mut bytes.as_slice())?;
    println!("decoded back: {decoded:?}");

    assert_eq!(decoded, player);
    println!("round-trip is lossless");

    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}
