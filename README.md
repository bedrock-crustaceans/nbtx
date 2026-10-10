<br />
<div align="center">

# nbtx

A Named Binary Tag (NBT) library for Minecraft, written in pure Rust.

[![rust][rust_badge_url]][rust_url]
[![format][format_badge_url]][format_url]
[![crates.io][crates_badge_url]][crates_url]
[![docs.rs][docs_badge_url]][docs_url]
[![license][license_badge_url]][license_url]

</div>

<!-- BADGES -->
[format_badge_url]: https://img.shields.io/badge/format-nbt-black?style=flat-square
[format_url]: https://minecraft.wiki/w/NBT_format

[rust_badge_url]: https://img.shields.io/badge/rust-2024-%23D34516?style=flat-square&logo=rust&logoColor=%23D34516&labelColor=white
[rust_url]: https://rust-lang.org/

[crates_badge_url]: https://img.shields.io/crates/v/nbtx?style=flat-square
[crates_url]: https://crates.io/crates/nbtx

[docs_badge_url]: https://img.shields.io/docsrs/nbtx?style=flat-square
[docs_url]: https://docs.rs/nbtx

[license_badge_url]: https://img.shields.io/github/license/bedrock-crustaceans/nbtx?style=flat-square
[license_url]: LICENSE
<!-- BADGES -->

nbtx reads and writes NBT, the tagged binary format Minecraft uses for worlds, items, entities and network payloads. It is built for **Bedrock Edition** first, but speaks every wire variant, so Java Edition files work too. Part of the [bedrock-crustaceans](https://github.com/bedrock-crustaceans) ecosystem.

### Features

- **every wire variant** - big-endian (Java, most `.nbt` files), little-endian (Bedrock on disk) and varint (Bedrock over the network), behind one API.
- **derive-based mapping** - turn plain Rust structs into NBT and back with `#[derive(Facet)]`; no hand-written visitors.
- **schemaless data** - the dynamic `nbtx::Value` tree for editors, converters and anything whose shape isn't known at compile time, with `to_value`/`from_value` to move between it and your own types without re-encoding.
- **strictly typed lists** - `Value::List` holds a `ValueList` with one variant per element type, so a heterogeneous list can't be built and an empty list keeps the element type it was decoded with.
- **lossless round-trips** - compound key order is preserved by default, and NBT strings are kept as raw bytes (`BString`), so files re-encode byte-for-byte even when they contain invalid UTF-8.
- **SNBT** - parse and print the human-readable text form used in commands and NBT editors.
- **safe on hostile input** - malformed, truncated or malicious data produces a typed `nbtx::Error`, never a panic. The decoder is fuzzed (see [`fuzz`](fuzz)).

### Installation

```toml
[dependencies]
nbtx = "4.0"
# `#[derive(Facet)]` expands to `::facet::` paths, so your crate needs facet itself.
# Keep it on the same version nbtx uses, or the derive targets a different trait.
facet = { version = "0.50.0-rc.7", features = ["bstr"] }
# Only needed for `BString` fields - NBT strings that may not be valid UTF-8.
bstr = "1"
```

### Usage

- structs:

    ```rust
    use facet::Facet;

    #[derive(Facet, Debug, PartialEq)]
    struct Player {
        name: String,
        level: i32,
        health: f32,
    }

    let player = Player { name: "Steve".to_owned(), level: 42, health: 19.5 };

    let bytes = nbtx::to_be_bytes(&player)?;
    let decoded: Player = nbtx::from_be_bytes(&mut bytes.as_slice())?;
    assert_eq!(decoded, player);
    ```

- bedrock:

    ```rust
    // `level.dat`, world data and other files on disk
    let disk = nbtx::to_le_bytes(&player)?;
    let player: Player = nbtx::from_le_bytes(&mut disk.as_slice())?;

    // packets on the wire - small ints and short strings shrink to a single byte
    let wire = nbtx::to_varint_bytes(&player)?;
    let player: Player = nbtx::from_varint_bytes(&mut wire.as_slice())?;
    ```

- dynamic values:

    ```rust
    use nbtx::{Compound, Value, ValueList};

    let item = Value::Compound(Compound::from_iter([
        ("id".into(), Value::String("minecraft:diamond_sword".into())),
        ("Count".into(), Value::Byte(1)),
        ("Lore".into(), Value::List(ValueList::Int(vec![1, 2, 3]))),
    ]));

    let bytes = nbtx::to_le_bytes(&item)?;
    let decoded: Value = nbtx::from_le_bytes(&mut bytes.as_slice())?;

    let count = decoded.as_compound().unwrap()[bstr::BStr::new("Count")].as_byte();
    ```

- snbt:

    ```rust
    let text = nbtx::to_string(&item)?;   // {id:"minecraft:diamond_sword",Count:1b,...}
    let parsed: Value = nbtx::from_string(&text)?;
    assert_eq!(parsed, item);
    ```

Decoders read from any `std::io::Read`, so a byte slice, a `File` or a socket all work as-is.

### Wire variants

NBT is one tag model with three byte layouts. Nothing in the data says which one it is, so you have to know what you're holding - exactly like Minecraft does.

| Variant | Encode | Decode | Used by |
| --- | --- | --- | --- |
| big-endian | `to_be_bytes` | `from_be_bytes` | Java Edition, most `.nbt` files |
| little-endian | `to_le_bytes` | `from_le_bytes` | Bedrock, on disk (`level.dat`, worlds) |
| varint | `to_varint_bytes` | `from_varint_bytes` | Bedrock, over the network |

Each pair is a thin alias over the generic `to_bytes::<E>`/`from_bytes::<E, T>`, for code that is itself generic over the variant.

### Cargo features

| Feature | Default | What it does |
| --- | --- | --- |
| `nbt` | yes | The binary codec. |
| `snbt` | yes | The SNBT text codec (`to_string`/`from_string`). |
| `preserve_order` | yes | Backs `Compound` with an `IndexMap` so key order survives a round-trip. Without it, `Compound` is a sorted `BTreeMap` and the `indexmap` dependency is dropped. |
| `error-context` | no | Adds extra context to decoding errors. |

`to_value`/`from_value` need no features at all.

### Examples

Every example is runnable and explains what it's doing as it goes. Start with `hello_world`.

```sh
cargo run --example hello_world
cargo run --example snbt --features snbt
```

| Example | What it covers |
| --- | --- |
| [`hello_world`](examples/hello_world.rs) | The thirty-second introduction: derive, encode, decode. |
| [`structs`](examples/structs.rs) | Which Rust type maps to which NBT tag; `Option`, nesting, renaming, map fields, and the mapping's limits. |
| [`endianness`](examples/endianness.rs) | The three wire variants and when to use which. |
| [`value`](examples/value.rs) | The dynamic `Value` tree: building NBT without a schema, the `as_*`/`is_*`/`into_*` accessors, key order. |
| [`value_conversion`](examples/value_conversion.rs) | `to_value`/`from_value`: struct ⇄ `Value` directly, with no bytes in between. |
| [`named_root`](examples/named_root.rs) | The document root name and the `Named<T>` wrapper that preserves it. |
| [`non_utf8`](examples/non_utf8.rs) | NBT strings are raw bytes; `bstr::BString` fields keep them losslessly. |
| [`unknown_fields`](examples/unknown_fields.rs) | Unknown compound keys are an error by default, plus the `#[facet(nbtx::allow_unknown_fields)]` opt-out. |
| [`errors`](examples/errors.rs) | Malformed, truncated and hostile input, and the typed `nbtx::Error` each one produces. |
| [`snbt`](examples/snbt.rs) | The human-readable SNBT text form. |
| [`server_dat`](examples/server_dat.rs) | Decoding a real Minecraft `servers.dat`, with a schema and without. |
| [`in_writer`](examples/in_writer.rs) | Embedding NBT inside a larger byte stream, and reading it back out. |

### Contributing

Bug fixes, features, docs and tests are all welcome - see [CONTRIBUTING.md](CONTRIBUTING.md) to get started.

### License

Licensed under the [Apache License 2.0](LICENSE).
