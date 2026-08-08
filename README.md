
# nbtx

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE) [![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)

A high-performance, pure-Rust implementation of the **Named Binary Tag (NBT)** format, specifically tailored for **Minecraft Bedrock Edition**. Part of the [bedrock-crustaceans](https://github.com/bedrock-crustaceans) ecosystem.

---

## 🚀 Features

* **Bedrock Optimized:** Full support for Little Endian and Varint encoding used in Bedrock.
* **Memory Efficient:** Optimized for low-allocation parsing and high-speed serialization.
* **Unstructed Data**: Support for NBT data with no predefined structure via `nbtx::Value`, and `nbtx::to_value`/`nbtx::from_value` to convert between it and your own types without re-encoding.
* **Facet Reflection:** Map between Rust structs and NBT with a single `#[derive(nbtx::Facet)]`.
* **Strictly Typed:** Safe handling of Compound, List, and Byte Array tags.

## 📦 Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
nbtx = "4.0"
# Pin facet to the same rev nbtx does. `#[derive(Facet)]` expands to `::facet::`
# paths, so the deriving crate needs facet itself — and a different version would
# derive a different trait than nbtx's signatures are bounded on.
facet = { git = "https://github.com/facet-rs/facet", rev = "93e597980313f53ce81d242910adc9bdf34f9665", features = ["bstr"] }
# Only needed for `BString` fields — NBT strings that may not be valid UTF-8.
bstr = "1"
```

and then start deserialising with `nbtx`. Types are mapped to NBT via `#[derive(facet::Facet)]`.

## 📖 Examples

Every example is runnable and prints an explanation of what it is doing. Start with `hello_world`, then pick whatever you need.

```sh
cargo run --example hello_world
cargo run --example snbt --features snbt
```

| Example | What it covers |
| --- | --- |
| [`hello_world`](examples/hello_world.rs) | The thirty-second introduction: derive, encode, decode. |
| [`structs`](examples/structs.rs) | Which Rust type maps to which NBT tag; `Option`, nesting, renaming, map fields, and the mapping's limits. |
| [`endianness`](examples/endianness.rs) | The three wire variants — big-endian, little-endian, varint — and when to use which. |
| [`value`](examples/value.rs) | The dynamic `Value` tree: building NBT without a schema, the `as_*`/`is_*`/`into_*` accessors, key order. |
| [`value_conversion`](examples/value_conversion.rs) | `to_value`/`from_value`: struct ⇄ `Value` directly, with no bytes in between (needs no features). |
| [`named_root`](examples/named_root.rs) | The document root name and the `Named<T>` wrapper that preserves it. |
| [`non_utf8`](examples/non_utf8.rs) | NBT strings are raw bytes; `bstr::BString` fields keep them losslessly. |
| [`unknown_fields`](examples/unknown_fields.rs) | Unknown compound keys are an error by default, plus the `#[facet(nbtx::allow_unknown_fields)]` opt-out. |
| [`errors`](examples/errors.rs) | Malformed, truncated and hostile input, and the typed `nbtx::Error` each one produces. |
| [`snbt`](examples/snbt.rs) | The human-readable SNBT text form (`--features snbt`). |
| [`server_dat`](examples/server_dat.rs) | Decoding a real Minecraft `servers.dat`, both with a schema and without. |
| [`in_writer`](examples/in_writer.rs) | Embedding NBT inside a larger byte stream, and reading it back out. |

## Contributing

We welcome contributions of all kinds, including bug fixes, new features, docs updates, and improvements across crates.  
Please read the full contribution guide here: **[CONTRIBUTING.md](CONTRIBUTING.md)**  

For guidance or collaboration, connect with the community on Discord.

## License

This project is licensed under the **Apache License 2.0**. See the [LICENSE](https://github.com/bedrock-crustaceans/nbtx/tree/master/LICENSE) for the details.
