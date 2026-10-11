//! SNBT: the human-readable text form of NBT, as typed into Minecraft commands.
//!
//! `cargo run --example snbt --features snbt`
//!
//! SNBT is what you see in `/give @s stone{display:{Name:"..."}}` and in most
//! NBT editors. It is the same tag model as the binary format wearing a
//! JSON-like syntax, with one addition JSON has no need for: because the tag is
//! not written out, scalars carry a **type suffix** — `1b` is a Byte, `1s` a
//! Short, plain `1` an Int, `1l` a Long, `1f` a Float, `1d` a Double — and the
//! typed arrays are spelled `[B;...]`, `[I;...]` and `[L;...]` to tell them
//! apart from a plain `[...]` list.
//!
//! `to_string`/`from_string` accept the same types as the binary codec, so
//! anything you can encode you can also print, and vice versa.

use nbtx::{Compound, Value, ValueList};

fn main() -> Result<(), nbtx::Error> {
    let item = Value::Compound(Compound::from_iter([
        ("id".into(), Value::String("minecraft:diamond_sword".into())),
        ("Count".into(), Value::Byte(1)),
        (
            "tag".into(),
            Value::Compound(Compound::from_iter([
                ("Damage".into(), Value::Short(4)),
                ("Unbreakable".into(), Value::Byte(1)),
                (
                    "Enchantments".into(),
                    Value::List(ValueList::Compound(vec![
                        Compound::from_iter([
                            ("id".into(), Value::String("minecraft:sharpness".into())),
                            ("lvl".into(), Value::Short(5)),
                        ]),
                        Compound::from_iter([
                            ("id".into(), Value::String("minecraft:unbreaking".into())),
                            ("lvl".into(), Value::Short(3)),
                        ]),
                    ])),
                ),
                ("UUID".into(), Value::IntArray(vec![1, 2, 3, 4])),
                ("Icon".into(), Value::ByteArray(vec![0xde, 0xad])),
                ("Longs".into(), Value::LongArray(vec![-1, 1])),
                ("Weight".into(), Value::Float(0.5)),
                ("Precise".into(), Value::Double(0.5)),
            ])),
        ),
    ]));

    let text = nbtx::to_string(&item)?;
    println!("a nested compound rendered as SNBT:\n  {text}");

    // Parsing is the exact inverse: the suffixes are what let the parser
    // recover the tag, which is why `Count:1b` must not be written `Count:1`.
    let parsed: Value = nbtx::from_string(&text)?;
    assert_eq!(parsed, item);
    println!("\nparsed back into an identical `Value`");

    // The parser is deliberately more tolerant than the writer: whitespace
    // anywhere, trailing commas, and upper- or lower-case suffixes, because
    // SNBT in the wild is usually hand-written.
    let hand_written = r#"{
        id : "minecraft:stone" ,
        Count : 64B ,
        tag : { Damage : 0S , },
    }"#;
    let lenient: Value = nbtx::from_string(hand_written)?;
    println!("\nhand-written SNBT with spaces, trailing commas and `64B`:");
    println!("  parsed to {}", nbtx::to_string(&lenient)?);

    // Derived structs work the same way, which makes SNBT a convenient
    // debugging printer for your own types.
    #[derive(facet::Facet, Debug, PartialEq)]
    struct Recipe {
        name: String,
        ingredients: Vec<String>,
        counts: Vec<i32>,
        secret: bool,
    }
    let recipe = Recipe {
        name: "Rusty Burger".to_owned(),
        ingredients: vec!["bun".to_owned(), "patty".to_owned()],
        counts: vec![1, 2],
        secret: false,
    };
    let recipe_text = nbtx::to_string(&recipe)?;
    println!("\na derived struct as SNBT:\n  {recipe_text}");
    let recipe_back: Recipe = nbtx::from_string(&recipe_text)?;
    assert_eq!(recipe_back, recipe);
    println!("  and back again, unchanged");

    // Binary and text are two renderings of one tag tree, so converting is just
    // decode-then-encode. This is the whole of an `nbt2snbt` tool.
    let binary = nbtx::to_be_bytes(&item)?;
    let from_binary: Value = nbtx::from_be_bytes(&mut binary.as_slice())?;
    assert_eq!(nbtx::to_string(&from_binary)?, text);
    println!(
        "\n{} bytes of binary NBT and {} characters of SNBT",
        binary.len(),
        text.len()
    );
    println!("describe the same tree; converting is decode-then-encode.");

    // One asymmetry to know about: SNBT is text, so a `Value::String` holding
    // bytes that are not UTF-8 cannot be represented faithfully and is rendered
    // lossily. Keep the binary format for those (see `examples/non_utf8.rs`).
    let raw = Value::String(bstr::BString::from(vec![0xff, 0xfe, b'h', b'i']));
    println!(
        "\nnon-UTF-8 strings are rendered lossily: {}",
        nbtx::to_string(&raw)?
    );

    // Bad syntax comes back as a typed error, in the same `nbtx::Error` enum the
    // binary codec uses.
    match nbtx::from_string::<Value>("{unterminated: 1") {
        Ok(value) => unreachable!("must not parse: {value:?}"),
        Err(err) => println!("\nmalformed SNBT: {err}"),
    }

    Ok(())
}
