//! `#[derive(Facet)]` attributes and field shapes, as nbtx interprets them.
//!
//! `struct_roundtrip.rs` establishes that the derive path works; this file is
//! about the *mapping* it chooses — which wire key a field gets, which tag a
//! Rust type becomes, and how the container shapes (`Option`, `Vec`, arrays,
//! maps, nested structs, enums) compose. Those choices are the crate's schema
//! contract: changing one silently re-keys every document a user has on disk.
//!
//! Each attribute is asserted at the *key* level — by decoding into a `Value`
//! and looking at the compound — rather than only through a self round-trip,
//! which would pass even if the encoder and decoder agreed on the wrong name.

#![cfg(feature = "nbt")]

use bstr::BString;
use facet::Facet;
use nbtx::{
    Compound, Value, from_be_bytes, from_le_bytes, from_varint_bytes, to_be_bytes, to_le_bytes,
    to_varint_bytes,
};
use std::collections::{BTreeMap, HashMap};

macro_rules! for_each_endian {
    ($check:ident) => {
        $check!(to_be_bytes, from_be_bytes);
        $check!(to_le_bytes, from_le_bytes);
        $check!(to_varint_bytes, from_varint_bytes);
    };
}

/// The compound keys a value encodes to, in wire order.
fn keys_of<'f>(v: &'f (impl Facet<'f> + ?Sized)) -> Vec<String> {
    let bytes = to_be_bytes(v).unwrap();
    let decoded: Value = from_be_bytes(&mut bytes.as_slice()).unwrap();
    decoded
        .as_compound()
        .unwrap()
        .keys()
        .map(ToString::to_string)
        .collect()
}

/// The `Value` a struct encodes to, for asserting per-field tags.
fn as_value<'f>(v: &'f (impl Facet<'f> + ?Sized)) -> Value {
    let bytes = to_be_bytes(v).unwrap();
    from_be_bytes(&mut bytes.as_slice()).unwrap()
}

fn get<'a>(v: &'a Value, key: &str) -> &'a Value {
    v.as_compound().unwrap().get(&BString::from(key)).unwrap()
}

// --- naming ---------------------------------------------------------------

/// Every `rename_all` style nbtx is expected to support, asserted by the wire
/// key it produces. Only `camelCase` and `PascalCase` appear elsewhere in the
/// suite, and neither is checked against the actual key.
#[test]
fn every_rename_all_style_produces_its_wire_key() {
    #[derive(Facet)]
    #[facet(rename_all = "camelCase")]
    struct Camel {
        some_field_name: i32,
    }
    #[derive(Facet)]
    #[facet(rename_all = "PascalCase")]
    struct Pascal {
        some_field_name: i32,
    }
    #[derive(Facet)]
    #[facet(rename_all = "SCREAMING_SNAKE_CASE")]
    struct Screaming {
        some_field_name: i32,
    }
    #[derive(Facet)]
    #[facet(rename_all = "kebab-case")]
    struct Kebab {
        some_field_name: i32,
    }
    #[derive(Facet)]
    struct Plain {
        some_field_name: i32,
    }

    assert_eq!(keys_of(&Camel { some_field_name: 1 }), ["someFieldName"]);
    assert_eq!(keys_of(&Pascal { some_field_name: 1 }), ["SomeFieldName"]);
    assert_eq!(
        keys_of(&Screaming { some_field_name: 1 }),
        ["SOME_FIELD_NAME"]
    );
    assert_eq!(keys_of(&Kebab { some_field_name: 1 }), ["some-field-name"]);
    // With no attribute the Rust identifier is used verbatim.
    assert_eq!(keys_of(&Plain { some_field_name: 1 }), ["some_field_name"]);
}

/// A field-level `rename` overrides the container's `rename_all` for that field
/// only — the escape hatch for a single key that does not follow the file's
/// convention.
#[test]
fn a_field_rename_overrides_rename_all_for_that_field_only() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename_all = "PascalCase")]
    struct S {
        follows_convention: i32,
        #[facet(rename = "created-on")]
        breaks_convention: i64,
    }
    let s = S {
        follows_convention: 1,
        breaks_convention: 2,
    };
    assert_eq!(keys_of(&s), ["FollowsConvention", "created-on"]);

    // ...and the renamed key is what the *decoder* matches on, too.
    let bytes = to_be_bytes(&s).unwrap();
    let back: S = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(back, s);
}

/// A renamed field must be matched by its effective name in the textual codec as
/// well — the two codecs resolve names through separate code paths.
#[cfg(feature = "snbt")]
#[test]
fn renames_apply_to_the_textual_codec_too() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename_all = "camelCase")]
    struct S {
        some_field: i32,
        #[facet(rename = "x")]
        other_field: i8,
    }
    let s = S {
        some_field: 1,
        other_field: 2,
    };
    assert_eq!(nbtx::to_string(&s).unwrap(), "{someField:1,x:2b}");
    let back: S = nbtx::from_string("{someField:1,x:2b}").unwrap();
    assert_eq!(back, s);
}

/// A container-level `rename` must **not** reach the binary wire format: the
/// root name is always empty, so renaming a Rust type cannot change the bytes on
/// disk. `root_handling` asserts the empty root name; this asserts that the
/// attribute specifically is inert.
#[test]
fn a_container_rename_does_not_change_the_wire_format() {
    #[derive(Facet)]
    #[facet(rename = "SomethingElse")]
    struct Renamed {
        a: i32,
    }
    #[derive(Facet)]
    struct NotRenamed {
        a: i32,
    }
    assert_eq!(
        to_be_bytes(&Renamed { a: 1 }).unwrap(),
        to_be_bytes(&NotRenamed { a: 1 }).unwrap(),
        "a container rename must be invisible on the wire"
    );
}

/// Fields are written in declaration order, but decoding must not depend on it:
/// a document whose keys arrive in any order still fills the right fields.
#[test]
fn decoding_does_not_depend_on_the_order_keys_arrive_in() {
    #[derive(Facet, Debug, PartialEq)]
    struct S {
        a: i32,
        b: i16,
        c: String,
    }
    assert_eq!(
        keys_of(&S {
            a: 1,
            b: 2,
            c: String::new()
        }),
        ["a", "b", "c"]
    );

    // The same three entries, written back to front.
    let reordered = Value::Compound(Compound::from([
        (BString::from("c"), Value::String(BString::from("z"))),
        (BString::from("b"), Value::Short(2)),
        (BString::from("a"), Value::Int(1)),
    ]));
    let bytes = to_be_bytes(&reordered).unwrap();
    let s: S = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(
        s,
        S {
            a: 1,
            b: 2,
            c: "z".to_owned()
        }
    );
}

// --- Option -----------------------------------------------------------------

/// `Option<Vec<T>>` must distinguish "the key was absent" from "the key held an
/// empty list" — collapsing the two would lose information that a schema
/// migration depends on.
#[test]
fn option_of_a_vec_distinguishes_none_from_an_empty_vec() {
    #[derive(Facet, Debug, PartialEq)]
    struct S {
        items: Option<Vec<i32>>,
    }
    macro_rules! check {
        ($to:ident, $from:ident) => {{
            for value in [None, Some(vec![]), Some(vec![1, 2])] {
                let s = S {
                    items: value.clone(),
                };
                let bytes = $to(&s).unwrap();
                let back: S = $from(&mut bytes.as_slice()).unwrap();
                assert_eq!(back, s);
            }
        }};
    }
    for_each_endian!(check);

    // `None` omits the key entirely; `Some(vec![])` writes an empty IntArray.
    assert!(keys_of(&S { items: None }).is_empty());
    assert_eq!(
        keys_of(&S {
            items: Some(vec![])
        }),
        ["items"]
    );
    assert!(
        get(
            &as_value(&S {
                items: Some(vec![])
            }),
            "items"
        )
        .is_int_array()
    );
}

/// An `Option` around a nested struct is the common "this sub-compound may be
/// absent" shape, and the `Some` case must still write a full compound.
#[test]
fn option_of_a_nested_struct_roundtrips_in_both_states() {
    #[derive(Facet, Debug, PartialEq)]
    struct Inner {
        n: i32,
    }
    #[derive(Facet, Debug, PartialEq)]
    struct S {
        inner: Option<Inner>,
        always: i8,
    }
    macro_rules! check {
        ($to:ident, $from:ident) => {{
            for inner in [None, Some(Inner { n: 7 })] {
                let s = S { inner, always: 1 };
                let bytes = $to(&s).unwrap();
                let back: S = $from(&mut bytes.as_slice()).unwrap();
                assert_eq!(back, s);
            }
        }};
    }
    for_each_endian!(check);

    assert_eq!(
        keys_of(&S {
            inner: None,
            always: 1
        }),
        ["always"]
    );
    let v = as_value(&S {
        inner: Some(Inner { n: 7 }),
        always: 1,
    });
    assert!(get(&v, "inner").is_compound());
    assert_eq!(get(get(&v, "inner"), "n"), &Value::Int(7));
}

/// `Option` fields *inside* a nested struct must behave the same as at the top
/// level — the omit-and-restore logic is per-struct, so a nested one exercises
/// it a second time.
#[test]
fn option_fields_nested_inside_another_struct() {
    #[derive(Facet, Debug, PartialEq)]
    struct Inner {
        maybe: Option<String>,
        always: i32,
    }
    #[derive(Facet, Debug, PartialEq)]
    struct Outer {
        inner: Inner,
    }
    for maybe in [None, Some("x".to_owned())] {
        let o = Outer {
            inner: Inner {
                maybe: maybe.clone(),
                always: 1,
            },
        };
        let bytes = to_le_bytes(&o).unwrap();
        let back: Outer = from_le_bytes(&mut bytes.as_slice()).unwrap();
        assert_eq!(back, o);
    }
    let omitted = as_value(&Outer {
        inner: Inner {
            maybe: None,
            always: 1,
        },
    });
    assert_eq!(get(&omitted, "inner").as_compound().unwrap().len(), 1);
}

/// An `Option<Value>` combines the two special cases — the dynamic downcast and
/// the omit-if-`None` rule — which are handled by different branches of the
/// serializer.
#[test]
fn option_of_a_dynamic_value_roundtrips() {
    #[derive(Facet, Debug, PartialEq)]
    struct S {
        v: Option<Value>,
    }
    for v in [
        None,
        Some(Value::LongArray(vec![1, 2])),
        Some(Value::List(vec![])),
    ] {
        let s = S { v: v.clone() };
        let bytes = to_be_bytes(&s).unwrap();
        let back: S = from_be_bytes(&mut bytes.as_slice()).unwrap();
        assert_eq!(back, s);
    }
    // The dynamic tag survives the `Option` wrapper.
    let v = as_value(&S {
        v: Some(Value::LongArray(vec![1])),
    });
    assert!(get(&v, "v").is_long_array());
}

/// Several `Option` fields in one struct, all `None`, must produce a compound
/// with *no* entries — not a compound of nulls, which NBT has no way to express.
#[test]
fn a_struct_of_all_none_options_encodes_an_empty_compound() {
    #[derive(Facet, Debug, PartialEq)]
    struct S {
        a: Option<i32>,
        b: Option<String>,
        c: Option<Vec<i64>>,
    }
    let s = S {
        a: None,
        b: None,
        c: None,
    };
    let bytes = to_be_bytes(&s).unwrap();
    assert_eq!(
        bytes,
        [0x0a, 0x00, 0x00, 0x00],
        "root compound, then TAG_End"
    );
    let back: S = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(back, s);
}

// --- containers of structs --------------------------------------------------

/// A `Vec` of structs becomes a `List` of compounds — the shape `servers.dat`
/// and every entity list uses.
#[test]
fn a_vec_of_structs_becomes_a_list_of_compounds() {
    #[derive(Facet, Debug, PartialEq)]
    struct Item {
        id: String,
        count: i8,
    }
    #[derive(Facet, Debug, PartialEq)]
    struct Inventory {
        items: Vec<Item>,
    }
    let inv = Inventory {
        items: vec![
            Item {
                id: "stone".to_owned(),
                count: 64,
            },
            Item {
                id: "dirt".to_owned(),
                count: 1,
            },
        ],
    };
    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&inv).unwrap();
            let back: Inventory = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, inv);
        }};
    }
    for_each_endian!(check);

    let v = as_value(&inv);
    let list = get(&v, "items").as_list().unwrap();
    assert_eq!(list.len(), 2);
    assert!(list[0].is_compound());
    assert_eq!(get(&list[0], "id"), &Value::String(BString::from("stone")));
}

/// A fixed-size array of structs uses a different reader (`init_array` plus
/// positional fields) from a `Vec`, so it needs its own case.
#[test]
fn an_array_of_structs_roundtrips() {
    #[derive(Facet, Debug, PartialEq)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[derive(Facet, Debug, PartialEq)]
    struct Shape {
        corners: [Point; 3],
    }
    let s = Shape {
        corners: [
            Point { x: 0, y: 0 },
            Point { x: 1, y: 0 },
            Point { x: 0, y: 1 },
        ],
    };
    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&s).unwrap();
            let back: Shape = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, s);
        }};
    }
    for_each_endian!(check);
}

/// A fixed-size array's length is part of its type, so a document with the wrong
/// number of elements must be rejected rather than silently truncated or padded.
#[test]
fn a_fixed_size_array_rejects_a_length_mismatch() {
    #[derive(Facet, Debug)]
    struct S {
        a: [i32; 3],
    }
    for wrong in [vec![1, 2], vec![1, 2, 3, 4]] {
        let doc = Value::Compound(Compound::from([(
            BString::from("a"),
            Value::IntArray(wrong.clone()),
        )]));
        let bytes = to_be_bytes(&doc).unwrap();
        assert!(
            from_be_bytes::<S>(&mut bytes.as_slice()).is_err(),
            "{} elements must not fill a [i32; 3]",
            wrong.len()
        );
    }
    // The right length succeeds, so the rejection is about the count.
    let doc = Value::Compound(Compound::from([(
        BString::from("a"),
        Value::IntArray(vec![1, 2, 3]),
    )]));
    let bytes = to_be_bytes(&doc).unwrap();
    assert_eq!(
        from_be_bytes::<S>(&mut bytes.as_slice()).unwrap().a,
        [1, 2, 3]
    );
}

/// Three levels of struct nesting, to confirm the compound reader re-enters
/// itself correctly rather than only handling one level of indirection.
#[test]
fn three_levels_of_struct_nesting_roundtrip() {
    #[derive(Facet, Debug, PartialEq)]
    struct Level3 {
        leaf: String,
    }
    #[derive(Facet, Debug, PartialEq)]
    struct Level2 {
        third: Level3,
        n: i16,
    }
    #[derive(Facet, Debug, PartialEq)]
    struct Level1 {
        second: Level2,
        flag: bool,
    }
    let v = Level1 {
        second: Level2 {
            third: Level3 {
                leaf: "bottom".to_owned(),
            },
            n: -1,
        },
        flag: true,
    };
    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&v).unwrap();
            let back: Level1 = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, v);
        }};
    }
    for_each_endian!(check);

    let doc = as_value(&v);
    assert_eq!(
        get(get(get(&doc, "second"), "third"), "leaf"),
        &Value::String(BString::from("bottom"))
    );
}

// --- maps -------------------------------------------------------------------

/// Both standard map types map to `Compound`, and both survive a round-trip.
/// `BTreeMap` additionally fixes the key order, so its encoding is
/// deterministic.
#[test]
fn both_standard_map_types_become_compounds() {
    #[derive(Facet, Debug, PartialEq)]
    struct S {
        btree: BTreeMap<String, i32>,
        hash: HashMap<String, i32>,
    }
    let s = S {
        btree: [("b".to_owned(), 2), ("a".to_owned(), 1)]
            .into_iter()
            .collect(),
        hash: [("x".to_owned(), 9)].into_iter().collect(),
    };
    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&s).unwrap();
            let back: S = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, s);
        }};
    }
    for_each_endian!(check);

    let v = as_value(&s);
    assert!(get(&v, "btree").is_compound());
    assert!(get(&v, "hash").is_compound());
    // A `BTreeMap` iterates sorted, so its key order on the wire is fixed.
    let keys: Vec<String> = get(&v, "btree")
        .as_compound()
        .unwrap()
        .keys()
        .map(ToString::to_string)
        .collect();
    assert_eq!(keys, ["a", "b"]);
}

/// A map accepts any key by definition, so the unknown-field denial that applies
/// to structs must **not** apply to it — otherwise a `HashMap` field could only
/// ever hold keys the Rust type already knew about, which is a contradiction.
#[test]
fn a_map_field_accepts_keys_no_struct_field_could_match() {
    #[derive(Facet, Debug, PartialEq)]
    struct S {
        extras: HashMap<String, i32>,
    }
    let doc = Value::Compound(Compound::from([(
        BString::from("extras"),
        Value::Compound(Compound::from([
            (BString::from("anything"), Value::Int(1)),
            (BString::from("at_all"), Value::Int(2)),
        ])),
    )]));
    let bytes = to_be_bytes(&doc).unwrap();
    let s: S = from_be_bytes(&mut bytes.as_slice()).expect("a map takes any key");
    assert_eq!(s.extras.len(), 2);
    assert_eq!(s.extras.get("anything"), Some(&1));
}

/// A map of structs, which nests the two container readers.
#[test]
fn a_map_of_structs_roundtrips() {
    #[derive(Facet, Debug, PartialEq)]
    struct Entry {
        n: i64,
    }
    #[derive(Facet, Debug, PartialEq)]
    struct S {
        entries: BTreeMap<String, Entry>,
    }
    let s = S {
        entries: [("k".to_owned(), Entry { n: -5 })].into_iter().collect(),
    };
    let bytes = to_varint_bytes(&s).unwrap();
    let back: S = from_varint_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(back, s);
}

// --- unknown fields ---------------------------------------------------------

/// `allow_unknown_fields` is a *container* attribute, so it must not leak into
/// nested structs: a lenient outer struct with a strict inner one still rejects
/// an unknown key inside the inner compound.
#[test]
fn allow_unknown_fields_is_not_inherited_by_a_nested_struct() {
    #[derive(Facet, Debug)]
    struct StrictInner {
        n: i32,
    }
    #[derive(Facet, Debug)]
    #[facet(nbtx::allow_unknown_fields)]
    struct LenientOuter {
        inner: StrictInner,
    }

    // An unknown key at the *outer* level is skipped...
    let ok = Value::Compound(Compound::from([
        (
            BString::from("inner"),
            Value::Compound(Compound::from([(BString::from("n"), Value::Int(1))])),
        ),
        (BString::from("unexpected"), Value::Byte(1)),
    ]));
    let bytes = to_be_bytes(&ok).unwrap();
    assert_eq!(
        from_be_bytes::<LenientOuter>(&mut bytes.as_slice())
            .unwrap()
            .inner
            .n,
        1
    );

    // ...but the same key inside the strict inner compound is not.
    let bad = Value::Compound(Compound::from([(
        BString::from("inner"),
        Value::Compound(Compound::from([
            (BString::from("n"), Value::Int(1)),
            (BString::from("unexpected"), Value::Byte(1)),
        ])),
    )]));
    let bytes = to_be_bytes(&bad).unwrap();
    match from_be_bytes::<LenientOuter>(&mut bytes.as_slice()) {
        Err(nbtx::Error::UnknownField(e)) => assert_eq!(e.container(), "StrictInner"),
        other => panic!("expected UnknownField from the inner struct, got {other:?}"),
    }
}

/// The SNBT deserializer denies unknown keys by default, exactly as the binary
/// one does: `src/snbt/de.rs::parse_struct_into` reads the same
/// `has_nbtx_attr(shape, "allow_unknown_fields")` marker. The two codecs must
/// agree about the same schema, or a document rejected on the wire would be
/// accepted as text (and quietly lose the extra key).
#[cfg(feature = "snbt")]
#[test]
fn snbt_denies_unknown_struct_keys_like_the_binary_codec() {
    #[derive(Facet, Debug)]
    struct Strict {
        a: i32,
    }
    match nbtx::from_string::<Strict>("{a:1,b:2}") {
        Err(nbtx::Error::UnknownField(e)) => {
            assert_eq!(e.field(), "b");
            assert_eq!(e.container(), "Strict");
        }
        other => panic!("SNBT must reject the unknown key `b`, got {other:?}"),
    }

    // The binary codec rejects the same document, with the same variant.
    let doc = Value::Compound(Compound::from([
        (BString::from("a"), Value::Int(1)),
        (BString::from("b"), Value::Int(2)),
    ]));
    let bytes = to_be_bytes(&doc).unwrap();
    assert!(matches!(
        from_be_bytes::<Strict>(&mut bytes.as_slice()),
        Err(nbtx::Error::UnknownField(_))
    ));
}

/// ...and the opt-out works in the textual codec too, including the rule that it
/// is a *container* attribute: a lenient outer struct does not make a strict
/// nested one lenient. An unknown key's value is parsed and discarded, so a
/// whole nested container may be skipped.
#[cfg(feature = "snbt")]
#[test]
fn snbt_allow_unknown_fields_opts_back_into_skipping() {
    #[derive(Facet, Debug)]
    struct StrictInner {
        n: i32,
    }
    #[derive(Facet, Debug)]
    #[facet(nbtx::allow_unknown_fields)]
    struct Lenient {
        a: i32,
        inner: StrictInner,
    }

    let v: Lenient =
        nbtx::from_string("{a:1,extra:{deep:[1,2,3]},inner:{n:2},trailing:\"x\"}").unwrap();
    assert_eq!(v.a, 1);
    assert_eq!(v.inner.n, 2);

    // The nested struct did not inherit the opt-out.
    match nbtx::from_string::<Lenient>("{a:1,inner:{n:2,nope:3}}") {
        Err(nbtx::Error::UnknownField(e)) => assert_eq!(e.container(), "StrictInner"),
        other => panic!("expected UnknownField from the inner struct, got {other:?}"),
    }
}

/// A map target accepts any key by definition, so the strict rule must not reach
/// it in the textual codec either.
#[cfg(feature = "snbt")]
#[test]
fn snbt_unknown_key_denial_does_not_apply_to_maps() {
    let m: BTreeMap<String, i32> = nbtx::from_string("{a:1,b:2}").unwrap();
    assert_eq!(m, [("a".to_owned(), 1), ("b".to_owned(), 2)].into());
}

// --- enums ------------------------------------------------------------------

/// A unit variant is matched by **name**, not by discriminant, so reordering the
/// variants in the Rust source cannot change how existing documents decode.
#[test]
fn unit_enum_variants_are_matched_by_name_not_by_position() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(nbtx::variant_as(str))]
    #[repr(u8)]
    enum First {
        Survival,
        Creative,
    }
    #[derive(Facet, Debug, PartialEq)]
    #[facet(nbtx::variant_as(str))]
    #[repr(u8)]
    enum Reordered {
        Creative,
        Survival,
    }
    #[derive(Facet, Debug, PartialEq)]
    struct A {
        mode: First,
    }
    #[derive(Facet, Debug, PartialEq)]
    struct B {
        mode: Reordered,
    }

    let bytes = to_be_bytes(&A {
        mode: First::Creative,
    })
    .unwrap();
    let back: B = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(
        back.mode,
        Reordered::Creative,
        "the variant must be selected by its name, not its index"
    );
}

/// A variant name the enum does not have is an error, not a silent default —
/// otherwise a typo in a hand-edited file would quietly change game behaviour.
#[test]
fn an_unknown_enum_variant_name_is_rejected() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(nbtx::variant_as(str))]
    #[repr(u8)]
    enum Mode {
        Survival,
    }
    #[derive(Facet, Debug)]
    struct S {
        mode: Mode,
    }
    let doc = Value::Compound(Compound::from([(
        BString::from("mode"),
        Value::String(BString::from("Sandbox")),
    )]));
    let bytes = to_be_bytes(&doc).unwrap();
    assert!(from_be_bytes::<S>(&mut bytes.as_slice()).is_err());
}

/// An enum field must arrive as a `String` tag; any other tag is a type error
/// rather than an attempt to coerce a number into a variant index.
#[test]
fn an_enum_field_requires_a_string_tag() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(nbtx::variant_as(str))]
    #[repr(u8)]
    enum Mode {
        Survival,
    }
    #[derive(Facet, Debug)]
    struct S {
        mode: Mode,
    }
    let doc = Value::Compound(Compound::from([(BString::from("mode"), Value::Int(0))]));
    let bytes = to_be_bytes(&doc).unwrap();
    match from_be_bytes::<S>(&mut bytes.as_slice()) {
        Err(nbtx::Error::UnexpectedType(e)) => {
            assert_eq!(e.expected(), nbtx::FieldType::String);
            assert_eq!(e.found(), nbtx::FieldType::Int);
        }
        other => panic!("expected UnexpectedType, got {other:?}"),
    }
}

// --- unusual struct shapes --------------------------------------------------

/// A tuple struct has no field names, so its positional indices become the wire
/// keys. Recorded because it is the only shape where the key is not a Rust
/// identifier.
#[test]
fn a_tuple_struct_uses_positional_keys() {
    #[derive(Facet, Debug, PartialEq)]
    struct Pair(i32, String);

    let p = Pair(7, "x".to_owned());
    assert_eq!(keys_of(&p), ["0", "1"]);
    let bytes = to_be_bytes(&p).unwrap();
    let back: Pair = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(back, p);
}

/// A unit struct carries no data, so it is an empty compound — the smallest
/// legal document nbtx can produce for a derived type.
#[test]
fn a_unit_struct_encodes_an_empty_compound() {
    #[derive(Facet, Debug, PartialEq)]
    struct Marker;

    assert_eq!(to_be_bytes(&Marker).unwrap(), [0x0a, 0x00, 0x00, 0x00]);
    let bytes = to_be_bytes(&Marker).unwrap();
    let back: Marker = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(back, Marker);
}

/// The `Vec<u8>` / `BString` distinction, in one struct: identical Rust
/// payloads, different NBT tags. This is the whole reason `is_bstring` exists,
/// and it is only observable when the two appear side by side.
#[test]
fn a_bstring_field_and_a_vec_u8_field_take_different_tags() {
    #[derive(Facet, Debug, PartialEq)]
    struct S {
        text: BString,
        blob: Vec<u8>,
    }
    let s = S {
        text: BString::from(vec![0x68u8, 0x69]),
        blob: vec![0x68, 0x69],
    };
    let v = as_value(&s);
    assert!(
        get(&v, "text").is_string(),
        "a BString field is a String tag"
    );
    assert!(
        get(&v, "blob").is_byte_array(),
        "a Vec<u8> field is a ByteArray tag"
    );
    // The payload bytes are identical; only the tag differs.
    assert_eq!(
        get(&v, "text").as_string().unwrap().as_slice(),
        get(&v, "blob").as_byte_array().unwrap().as_slice()
    );

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&s).unwrap();
            let back: S = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, s);
        }};
    }
    for_each_endian!(check);
}

/// A `BString` field is a `String` tag in the binary codec and a *quoted string*
/// in SNBT: `snbt::ser::render` and `snbt::de::parse_into` apply the same
/// `is_bstring` check as `nbt::ser`/`nbt::de`, so the documented
/// "`bstr::BString` → String (8)" convention holds in both codecs and a document
/// can be moved between them through the same Rust type.
#[cfg(feature = "snbt")]
#[test]
fn a_bstring_field_is_a_quoted_string_in_snbt_too() {
    #[derive(Facet, Debug, PartialEq)]
    struct S {
        text: BString,
    }
    let s = S {
        text: BString::from("hi"),
    };
    assert_eq!(nbtx::to_string(&s).unwrap(), r#"{text:"hi"}"#);
    let back: S = nbtx::from_string(r#"{text:"hi"}"#).unwrap();
    assert_eq!(back, s);

    // The dynamic rendering of the same struct (binary → Value → SNBT) is now
    // the *same text*, and parses back into the struct.
    let via_value = nbtx::to_string(&as_value(&s)).unwrap();
    assert_eq!(via_value, r#"{text:"hi"}"#);
    assert_eq!(nbtx::from_string::<S>(&via_value).unwrap(), s);

    // A `Vec<u8>` field is still a byte-array literal: only the string-shaped
    // type changed sides.
    #[derive(Facet, Debug, PartialEq)]
    struct Blob {
        blob: Vec<u8>,
    }
    assert_eq!(
        nbtx::to_string(&Blob {
            blob: vec![104, 105]
        })
        .unwrap(),
        "{blob:[B;104b,105b]}"
    );

    // A `Vec<BString>` is a List of String tags, not a list of byte arrays, in
    // the textual codec as well as the binary one.
    #[derive(Facet, Debug, PartialEq)]
    struct Names {
        names: Vec<BString>,
    }
    let names = Names {
        names: vec![BString::from("a"), BString::from("b")],
    };
    assert_eq!(nbtx::to_string(&names).unwrap(), r#"{names:["a","b"]}"#);
    assert_eq!(
        nbtx::from_string::<Names>(r#"{names:["a","b"]}"#).unwrap(),
        names
    );
}

/// `Named<T>`'s `name` is a `BString`, so it inherits the fix: a named root
/// survives an SNBT round-trip instead of turning into a byte array.
#[cfg(feature = "snbt")]
#[test]
fn a_named_root_roundtrips_through_snbt() {
    let named = nbtx::Named {
        name: BString::from("hello world"),
        value: 7i32,
    };
    let text = nbtx::to_string(&named).unwrap();
    assert_eq!(text, r#"{name:"hello world",value:7}"#);
    let back: nbtx::Named<i32> = nbtx::from_string(&text).unwrap();
    assert_eq!(back.name, named.name);
    assert_eq!(back.value, named.value);
}

/// SNBT is text, so a `BString` holding bytes that are not valid UTF-8 is
/// rendered lossily — the same rule `Value::String` follows. The tag is still a
/// string, which is the part that has to agree between the codecs.
#[cfg(feature = "snbt")]
#[test]
fn a_non_utf8_bstring_field_renders_lossily_like_value_string() {
    #[derive(Facet, Debug, PartialEq)]
    struct S {
        text: BString,
    }
    let s = S {
        text: BString::from(vec![0xffu8, 0xfe]),
    };
    let from_struct = nbtx::to_string(&s).unwrap();
    let from_value = nbtx::to_string(&as_value(&s)).unwrap();
    assert_eq!(from_struct, from_value);
    assert!(
        from_struct.starts_with("{text:\""),
        "still a quoted string: {from_struct}"
    );
}

/// A bare `u8` field encodes *and* decodes as a `Byte` tag. `nbt::ser::scalar_tag`
/// maps `ScalarType::U8` to `Byte` and `write_scalar` writes the byte, matching
/// `nbt::de::read_scalar` (which reads one back with `cast_unsigned`) and both
/// halves of the SNBT codec — the read and write halves of the binary codec used
/// to disagree about this one type.
#[test]
fn a_bare_u8_field_encodes_as_a_byte_tag() {
    #[derive(Facet, Debug, PartialEq)]
    struct S {
        a: u8,
    }

    // 200 is past `i8::MAX`, so it also pins the bit pattern rather than the
    // value: the `Byte` tag is signed on the wire.
    let s = S { a: 200 };
    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&s).expect("a u8 field must encode as a Byte tag");
            let back: S = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, s);
        }};
    }
    for_each_endian!(check);

    // It is the `Byte` tag, and the bytes are the ones nbtx already decoded
    // before it could encode them.
    assert!(get(&as_value(&s), "a").is_byte());
    let equivalent = Value::Compound(Compound::from([(
        BString::from("a"),
        Value::Byte(200u8.cast_signed()),
    )]));
    assert_eq!(to_be_bytes(&s).unwrap(), to_be_bytes(&equivalent).unwrap());

    // The whole `u8` range survives, and SNBT agrees with the binary codec.
    for n in [0u8, 1, 127, 128, 255] {
        let s = S { a: n };
        let bytes = to_be_bytes(&s).unwrap();
        assert_eq!(from_be_bytes::<S>(&mut bytes.as_slice()).unwrap(), s);
        #[cfg(feature = "snbt")]
        {
            let text = nbtx::to_string(&s).unwrap();
            assert_eq!(nbtx::from_string::<S>(&text).unwrap(), s);
        }
    }

    // A `Vec<u8>` still goes through the list path as a ByteArray, unchanged.
    #[derive(Facet, Debug, PartialEq)]
    struct Blob {
        a: Vec<u8>,
    }
    assert!(get(&as_value(&Blob { a: vec![200] }), "a").is_byte_array());
}

// --- one struct with everything ---------------------------------------------

/// Every supported field shape in a single struct, round-tripped in all three
/// variants and checked tag by tag. `struct_roundtrip::derive_tag_convention`
/// covers the scalars and simple containers; this adds the shapes it does not —
/// fixed-size arrays, both map types, `Option`, an enum, a `BString` and a
/// nested struct — so the whole convention is asserted in one place.
#[test]
fn one_struct_covering_every_supported_field_shape() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(nbtx::variant_as(str))]
    #[repr(u8)]
    enum Mode {
        Creative,
    }
    #[derive(Facet, Debug, PartialEq)]
    struct Inner {
        n: i32,
    }
    #[derive(Facet, Debug, PartialEq)]
    struct Everything {
        flag: bool,
        byte: i8,
        short: i16,
        int: i32,
        long: i64,
        float: f32,
        double: f64,
        text: String,
        raw: BString,
        blob: Vec<u8>,
        ints: Vec<i32>,
        longs: Vec<i64>,
        list: Vec<String>,
        fixed_bytes: [u8; 2],
        fixed_ints: [i32; 2],
        fixed_list: [String; 2],
        map: BTreeMap<String, i32>,
        nested: Inner,
        dynamic: Value,
        mode: Mode,
        maybe: Option<i32>,
        absent: Option<i32>,
    }

    let v = Everything {
        flag: true,
        byte: -1,
        short: -2,
        int: -3,
        long: -4,
        float: 1.5,
        double: 2.5,
        text: "text".to_owned(),
        raw: BString::from(vec![0xffu8, 0xfe]),
        blob: vec![1, 2],
        ints: vec![1, -2],
        longs: vec![3, -4],
        list: vec!["a".to_owned(), "b".to_owned()],
        fixed_bytes: [7, 8],
        fixed_ints: [9, 10],
        fixed_list: ["c".to_owned(), "d".to_owned()],
        map: [("k".to_owned(), 1)].into_iter().collect(),
        nested: Inner { n: 99 },
        dynamic: Value::LongArray(vec![1, 2]),
        mode: Mode::Creative,
        maybe: Some(5),
        absent: None,
    };

    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(&v).unwrap();
            let back: Everything = $from(&mut bytes.as_slice()).unwrap();
            assert_eq!(back, v);
        }};
    }
    for_each_endian!(check);

    let doc = as_value(&v);
    for (key, want) in [
        ("flag", nbtx::FieldType::Byte),
        ("byte", nbtx::FieldType::Byte),
        ("short", nbtx::FieldType::Short),
        ("int", nbtx::FieldType::Int),
        ("long", nbtx::FieldType::Long),
        ("float", nbtx::FieldType::Float),
        ("double", nbtx::FieldType::Double),
        ("text", nbtx::FieldType::String),
        ("raw", nbtx::FieldType::String),
        ("blob", nbtx::FieldType::ByteArray),
        ("ints", nbtx::FieldType::IntArray),
        ("longs", nbtx::FieldType::LongArray),
        ("list", nbtx::FieldType::List),
        ("fixed_bytes", nbtx::FieldType::ByteArray),
        ("fixed_ints", nbtx::FieldType::IntArray),
        ("fixed_list", nbtx::FieldType::List),
        ("map", nbtx::FieldType::Compound),
        ("nested", nbtx::FieldType::Compound),
        ("dynamic", nbtx::FieldType::LongArray),
        ("mode", nbtx::FieldType::String),
        ("maybe", nbtx::FieldType::Int),
    ] {
        assert_eq!(
            get(&doc, key).discriminant(),
            want as u8,
            "{key} must be a {want} tag"
        );
    }
    assert!(
        !doc.as_compound()
            .unwrap()
            .contains_key(&BString::from("absent")),
        "a None field must be omitted entirely"
    );
}
