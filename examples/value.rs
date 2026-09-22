//! The dynamic [`nbtx::Value`] tree: building NBT without a Rust schema,
//! inspecting it, and the tag fidelity a derived struct cannot give you.
//!
//! `cargo run --example value`
//!
//! Reach for `Value` when the shape of the data is not known at compile time
//! (an editor, a converter, a debugging dump), or when you must reproduce a
//! file byte-for-byte. Reach for a derived struct when it is known — see
//! `examples/structs.rs`.

use bstr::{BStr, BString};
use nbtx::{Compound, Value, ValueList};

fn main() -> Result<(), nbtx::Error> {
    // `Compound` is a type alias, so this code is identical whether the crate
    // was built with `preserve_order` (an `IndexMap`, the default) or without
    // it (a sorted `BTreeMap`). Only the resulting key order differs.
    let mut root = Compound::new();

    // One entry per tag. Note the deliberate distinctions: `ByteArray` (7) is
    // not a `List` of `Byte`, `IntArray` (11) is not a `List` of `Int`, and a
    // `String` (8) holds raw bytes rather than a Rust `String`.
    root.insert("byte".into(), Value::Byte(-1));
    root.insert("short".into(), Value::Short(300));
    root.insert("int".into(), Value::Int(70_000));
    root.insert("long".into(), Value::Long(5_000_000_000));
    root.insert("float".into(), Value::Float(1.5));
    root.insert("double".into(), Value::Double(2.25));
    root.insert("byte_array".into(), Value::ByteArray(vec![0xde, 0xad]));
    root.insert("string".into(), Value::String(BString::from("hello")));
    root.insert("list".into(), Value::List(ValueList::Int(vec![1, 2, 3])));
    root.insert(
        "compound".into(),
        Value::Compound(Compound::from_iter([
            ("x".into(), Value::Double(0.5)),
            ("y".into(), Value::Double(64.0)),
        ])),
    );
    root.insert("int_array".into(), Value::IntArray(vec![1, 2, 3]));
    root.insert("long_array".into(), Value::LongArray(vec![-1, -2]));

    let value = Value::Compound(root);

    // `discriminant` is the on-wire tag byte, so this listing is also the tag
    // table: Byte 1, Short 2, Int 3, Long 4, Float 5, Double 6, ByteArray 7,
    // String 8, List 9, Compound 10, IntArray 11, LongArray 12.
    println!("every NBT tag, as a `Value`:");
    for (key, entry) in value.as_compound().expect("built as a compound") {
        println!("  tag {:>2}  {key:<11} {entry:?}", entry.discriminant());
    }

    let bytes = nbtx::to_be_bytes(&value)?;
    let decoded: Value = nbtx::from_be_bytes(&mut bytes.as_slice())?;
    assert_eq!(decoded, value);
    println!(
        "\n{} bytes, and every tag survives the round-trip",
        bytes.len()
    );

    let compound = decoded.as_compound().expect("root is a compound");

    // Compound keys are `BString`, not `String`, because NBT does not promise
    // its strings are UTF-8 (see `examples/non_utf8.rs`). Look one up with a
    // `&BStr`, which any `&str` converts to for free.
    let get = |key: &str| &compound[BStr::new(key)];

    println!("\nthree ways to get at the contents:");

    // `as_*` borrows and returns `Option`, for the common "peek and branch" case.
    println!(
        "  as_int      borrows              -> {}",
        get("int").as_int().expect("an Int")
    );

    // `is_*` answers the type question alone, without extracting anything.
    println!(
        "  is_list     tests the tag        -> list: {}, byte_array: {}",
        get("list").is_list(),
        get("byte_array").is_list()
    );

    // `into_*` consumes and, on a mismatch, hands the original `Value` back in
    // the `Err` arm so nothing is dropped on the floor.
    match get("string").clone().into_string() {
        Ok(s) => println!("  into_string consumes             -> {s:?}"),
        Err(original) => println!("  into_string gave the value back  -> {original:?}"),
    }
    match get("string").clone().into_int() {
        Ok(i) => println!("  into_int    consumes             -> {i}"),
        Err(original) => println!(
            "  into_int    was wrong, so it returned the value unchanged (tag {})",
            original.discriminant()
        ),
    }

    // `Value` compares directly against the matching Rust scalar/slice, which
    // keeps assertions and lookups readable.
    assert!(get("int") == 70_000_i32);
    assert!(get("string") == "hello");
    assert!(get("int_array") == &[1, 2, 3][..]);
    println!("\n`Value` compares directly with `i32`, `&str`, `&[i32]`, ...");

    // Key order is data. NBT files carry keys in a meaningful order and
    // Minecraft's own tools preserve it, so `Value::Compound` is an `IndexMap`
    // under the default `preserve_order` feature: decoding and re-encoding a
    // file reproduces it byte-for-byte. Built with `--no-default-features`,
    // `Compound` becomes a sorted `BTreeMap`, which is smaller and dependency
    // free but re-orders keys, so exact re-encoding is lost.
    let ordered = Value::Compound(Compound::from_iter([
        ("zeta".into(), Value::Byte(1)),
        ("alpha".into(), Value::Byte(2)),
        ("middle".into(), Value::Byte(3)),
    ]));
    let reencoded: Value = nbtx::from_be_bytes(&mut nbtx::to_be_bytes(&ordered)?.as_slice())?;
    let keys: Vec<_> = reencoded
        .as_compound()
        .expect("a compound")
        .keys()
        .map(ToString::to_string)
        .collect();
    println!("\ninserted zeta, alpha, middle; read back {keys:?}");
    if cfg!(feature = "preserve_order") {
        println!("  (`preserve_order` is on: insertion order is on-disk order)");
    } else {
        println!("  (`preserve_order` is off: `Compound` is a sorted `BTreeMap`)");
    }

    // A `List` carries a single element-type byte for the whole list, so a
    // heterogeneous one cannot be written. nbtx rejects it up front rather than
    // emitting a stream that would decode into different data.
    // `Value::List` holds a `ValueList`, so the refusal happens where the list
    // is built rather than on the way out — the bad document never exists.
    match ValueList::try_from(vec![Value::Int(1), Value::String("two".into())]) {
        Ok(_) => unreachable!("a heterogeneous list must not be constructible"),
        Err(err) => println!("\nlists must be homogeneous: {err}"),
    }

    Ok(())
}
