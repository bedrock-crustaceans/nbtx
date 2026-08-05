//! [`nbtx::to_value`] and [`nbtx::from_value`]: moving between a typed struct
//! and the dynamic [`nbtx::Value`] tree *without* encoding anything.
//!
//! `cargo run --example value_conversion`
//!
//! `examples/structs.rs` shows the typed path (struct ⇄ bytes) and
//! `examples/value.rs` the dynamic one (`Value` ⇄ bytes). This example is the
//! edge between them: the direct conversion, with no bytes in the middle.
//!
//! Reach for it when you want to inspect or patch a document dynamically before
//! projecting it into a struct, when you want to build a `Value` from typed
//! data you already have, or when you simply want the tag table without paying
//! for a serialise/deserialise round trip.
//!
//! Unlike every other example here, this one needs no feature flags: the
//! conversion never touches the wire format, so it works under
//! `--no-default-features` too.

use std::collections::BTreeMap;

use bstr::{BStr, BString};
use facet::Facet;
use nbtx::Value;

#[derive(Facet, Debug, PartialEq)]
struct Player {
    name: String,
    // A `Vec<u8>` is a `ByteArray` (tag 7), never a list of bytes.
    inventory_bits: Vec<u8>,
    // A `Vec<i32>` is an `IntArray` (11); a `Vec<i64>` would be a `LongArray`.
    position_hash: Vec<i32>,
    health: f32,
    // A `None` field is simply absent from the compound.
    nickname: Option<String>,
    // Raw bytes: NBT strings are not promised to be UTF-8.
    raw_tag: BString,
    stats: BTreeMap<String, i64>,
    // A `Value` field passes through untouched, keeping the exact tag of every
    // child — the only way a derived struct can carry data whose shape it does
    // not know at compile time.
    extra: Value,
}

fn main() -> Result<(), nbtx::Error> {
    let player = Player {
        name: "Steve".to_owned(),
        inventory_bits: vec![0b1010_1010, 0x0f],
        position_hash: vec![-1, 0, 1],
        health: 20.0,
        nickname: None,
        raw_tag: BString::from(vec![0xff, 0xfe, b'o', b'k']),
        stats: [("mined".to_owned(), 1_234_567_890_123_i64)]
            .into_iter()
            .collect(),
        extra: Value::List(vec![Value::Double(0.5), Value::Double(64.0)]),
    };

    // --- struct -> Value ---------------------------------------------------

    let value = nbtx::to_value(&player)?;
    let compound = value.as_compound().expect("a struct becomes a Compound");

    // `discriminant()` is the on-wire tag byte, so this listing *is* the
    // type-to-tag table the binary codec uses. Note that no bytes were produced
    // to get here.
    println!("`to_value` derives one NBT tag per Rust type:");
    for (key, entry) in compound {
        println!("  tag {:>2}  {key}", entry.discriminant());
    }
    println!("\n`nickname` was `None`, so it is absent entirely: {}", {
        let present = compound.contains_key(BStr::new("nickname"));
        if present {
            "it is there?!"
        } else {
            "confirmed"
        }
    });

    // The tree is an ordinary `Value`, so every `Value` accessor applies.
    let health = compound[BStr::new("health")]
        .as_float()
        .expect("f32 -> Float");
    println!("health reads back as an f32: {health}");

    // --- Value -> struct ---------------------------------------------------

    // Patch the dynamic tree, then project it back into the typed struct. This
    // is the round trip the feature exists for: read once, edit dynamically,
    // return to typed code.
    // `into_compound` consumes the value and hands the map back; on a tag
    // mismatch it would return the original `Value` in the `Err` arm instead.
    let mut map = value
        .clone()
        .into_compound()
        .expect("the root is a compound");
    map.insert("health".into(), Value::Float(11.5));
    map.insert("nickname".into(), Value::String("Steve the Builder".into()));

    let updated: Player = nbtx::from_value(Value::Compound(map))?;
    println!(
        "\nafter patching the tree: health {}, nickname {:?}",
        updated.health, updated.nickname
    );

    // Nothing else changed — including the `Value` field's exact tags.
    assert_eq!(updated.extra, player.extra);
    assert_eq!(updated.raw_tag, player.raw_tag);
    println!("everything else survived unchanged, raw bytes and all");

    // --- the invariant -----------------------------------------------------

    // `to_value` is defined to agree with the binary codec node for node: the
    // tree it builds is the tree you would get by encoding and decoding again.
    // That is what makes it safe to mix the two paths in one program.
    #[cfg(feature = "nbt")]
    {
        let bytes = nbtx::to_be_bytes(&player)?;
        let decoded: Value = nbtx::from_be_bytes(&mut bytes.as_slice())?;
        assert_eq!(
            value, decoded,
            "to_value must match a decode of this value's own bytes"
        );
        println!(
            "\nthe same tree the {} encoded bytes decode to — but built directly",
            bytes.len()
        );
    }

    // --- errors ------------------------------------------------------------

    // A tag that cannot fill the target field is refused rather than coerced:
    // NBT's integer widths are distinct types, and quietly widening a Short
    // into an Int would lose the document's own idea of its schema.
    let wrong = Value::Compound(
        [
            (BString::from("name"), Value::Int(1)),
            (BString::from("health"), Value::Float(1.0)),
        ]
        .into_iter()
        .collect(),
    );
    match nbtx::from_value::<Player>(wrong) {
        Ok(_) => unreachable!("an Int cannot fill a String field"),
        Err(err) => println!("\ntag mismatches are errors, not coercions: {err}"),
    }

    // Unknown keys are refused too, so schema drift cannot silently drop data.
    // Opt a single struct out with `#[facet(nbtx::allow_unknown_fields)]`.
    #[derive(Facet, Debug)]
    struct Small {
        name: String,
    }
    let extra_key = Value::Compound(
        [
            (BString::from("name"), Value::String("Steve".into())),
            (BString::from("unexpected"), Value::Byte(1)),
        ]
        .into_iter()
        .collect(),
    );
    match nbtx::from_value::<Small>(extra_key) {
        Ok(_) => unreachable!("an unknown key must be refused by default"),
        Err(err) => println!("unknown keys are refused by default: {err}"),
    }

    Ok(())
}
