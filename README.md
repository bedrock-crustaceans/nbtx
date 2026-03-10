
# nbtx

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE) [![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)

A high-performance, pure-Rust implementation of the **Named Binary Tag (NBT)** format, specifically tailored for **Minecraft Bedrock Edition** but including support for Java Edition. Part of the [bedrock-crustaceans](https://github.com/bedrock-crustaceans) ecosystem.

---

## 🚀 Features

* **Bedrock Optimized:** Full support for Little Endian and Varint encoding used in Bedrock.
* **Memory Efficient:** Optimized for low-allocation parsing and high-speed serialization.
* **Unstructed Data**: Support for NBT data with no predefined structure.
* **Serde Integration:** Optional `serde` support for easy mapping between Rust structs and NBT.
* **Strictly Typed:** Safe handling of Compound, List, and Byte Array tags.

## 📦 Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
nbtx = "2.0"
```
and then start deserialising with `nbtx`. Or take a look at any of the other [`examples`](https://github.com/bedrock-crustaceans/nbtx/tree/master/examples)

## Contributing 
We welcome contributions of all kinds, including bug fixes, new features, docs updates, and improvements across crates.  
Please read the full contribution guide here: **[CONTRIBUTING.md](CONTRIBUTING.md)**  

For guidance or collaboration, connect with the community on Discord. 






## License
This project is licensed under the **Apache License 2.0**. See the [LICENSE](https://github.com/bedrock-crustaceans/nbtx/tree/master/LICENSE) for the details.
