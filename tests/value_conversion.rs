//! [`nbtx::to_value`] / [`nbtx::from_value`]: converting between a typed
//! `#[derive(Facet)]` value and the dynamic [`Value`] tree *without* going
//! through the binary format.
//!
//! The property that gives these two functions their meaning, and the backbone
//! of this file, is that they are indistinguishable from the byte-level codec
//! with the bytes removed:
//!
//! ```text
//! to_value(&v)  ==  from_be_bytes::<Value>(&mut to_be_bytes(&v)?.as_slice())?
//! from_value::<T>(to_value(&v)?)  ==  v
//! ```
//!
//! A `Value` produced here must carry the same tag on every node as one decoded
//! from real NBT bytes — otherwise the two paths have quietly forked, and a
//! document that round-trips through one would not round-trip through the other.
//! Every structural test below therefore asserts the equivalence directly rather
//! than merely asserting that the conversion "looks right".
//!
//! The conversion is feature-independent (it never touches the wire format), so
//! most of this file compiles with `--no-default-features`; only the tests that
//! *compare against* the binary codec are gated on `nbt`.

use std::collections::BTreeMap;

use bstr::BString;
use facet::Facet;
use nbtx::{Compound, Value, from_value, to_value};

// --- helpers ----------------------------------------------------------------

fn get<'a>(v: &'a Value, key: &str) -> &'a Value {
    v.as_compound()
        .unwrap_or_else(|| panic!("expected a compound, got tag {}", v.discriminant()))
        .get(&BString::from(key))
        .unwrap_or_else(|| panic!("missing key `{key}`"))
}

fn compound(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Compound(
        entries
            .into_iter()
            .map(|(k, v)| (BString::from(k), v))
            .collect::<Compound>(),
    )
}

/// The same tree, obtained the long way round: encode to big-endian NBT, then
/// decode the bytes back into a `Value`.
#[cfg(feature = "nbt")]
fn via_bytes<'f, T: Facet<'f> + ?Sized>(value: &'f T) -> Value {
    let bytes = nbtx::to_be_bytes(value).expect("encoding must succeed");
    nbtx::from_be_bytes::<Value>(&mut bytes.as_slice()).expect("decoding must succeed")
}

/// Asserts the central invariant for one value: `to_value` agrees with the
/// binary codec node for node, and `from_value` inverts it.
#[cfg(feature = "nbt")]
macro_rules! assert_matches_binary {
    ($ty:ty, $value:expr) => {{
        let value: $ty = $value;
        let direct = to_value(&value).expect("to_value must succeed");
        assert_eq!(
            direct,
            via_bytes(&value),
            "to_value must equal the tree decoded from this value's own bytes"
        );
        let back: $ty = from_value(direct).expect("from_value must succeed");
        assert_eq!(back, value, "from_value(to_value(v)) must return v");
    }};
}

// --- the full tag table -----------------------------------------------------

#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(nbtx::variant_as(str))]
#[repr(u8)]
enum Mode {
    Survival,
    Creative,
}

#[derive(Facet, Debug, Clone, PartialEq)]
struct Inner {
    id: i32,
    label: String,
}

/// One field per NBT tag, plus every special case the conversion has to know
/// about: a raw `BString`, a dynamic `Value`, a present and an absent `Option`,
/// a unit enum, a nested struct, a `Vec` of structs and a map.
#[derive(Facet, Debug, Clone, PartialEq)]
struct AllTags {
    flag: bool,
    byte: i8,
    ubyte: u8,
    short: i16,
    int: i32,
    long: i64,
    float: f32,
    double: f64,
    byte_array: Vec<u8>,
    text: String,
    raw: BString,
    list: Vec<String>,
    nested: Inner,
    structs: Vec<Inner>,
    map: BTreeMap<String, i32>,
    int_array: Vec<i32>,
    long_array: Vec<i64>,
    mode: Mode,
    dynamic: Value,
    present: Option<i32>,
    absent: Option<i32>,
}

fn sample() -> AllTags {
    AllTags {
        flag: true,
        byte: -8,
        ubyte: 200,
        short: -300,
        int: 70_000,
        long: 5_000_000_000,
        float: 1.5,
        double: -2.25,
        byte_array: vec![0xde, 0xad, 0xbe, 0xef],
        text: "hello".to_owned(),
        raw: BString::from(vec![0xff, 0xfe, b'o', b'k']),
        list: vec!["a".to_owned(), "bb".to_owned()],
        nested: Inner {
            id: 7,
            label: "inner".to_owned(),
        },
        structs: vec![
            Inner {
                id: 1,
                label: "one".to_owned(),
            },
            Inner {
                id: 2,
                label: "two".to_owned(),
            },
        ],
        map: [("b".to_owned(), 2), ("a".to_owned(), 1)]
            .into_iter()
            .collect(),
        int_array: vec![1, -2, 3],
        long_array: vec![-1, i64::MAX],
        mode: Mode::Creative,
        dynamic: compound([
            ("kept", Value::LongArray(vec![9, 8])),
            ("deep", Value::List(vec![Value::Byte(1), Value::Byte(2)])),
        ]),
        present: Some(42),
        absent: None,
    }
}

/// The headline invariant, over a value that exercises every tag at once.
#[cfg(feature = "nbt")]
#[test]
fn every_tag_matches_the_binary_codec_and_round_trips() {
    assert_matches_binary!(AllTags, sample());
}

/// The type→tag table itself, asserted tag by tag on the tree `to_value`
/// produces. `discriminant()` is the on-wire tag byte, so this is the same
/// table the binary serializer's module docs state.
#[test]
fn each_rust_type_maps_to_its_documented_tag() {
    let v = to_value(&sample()).unwrap();
    for (key, tag) in [
        ("flag", 1u8),
        ("byte", 1),
        ("ubyte", 1),
        ("short", 2),
        ("int", 3),
        ("long", 4),
        ("float", 5),
        ("double", 6),
        ("byte_array", 7),
        ("text", 8),
        ("raw", 8),
        ("list", 9),
        ("nested", 10),
        ("structs", 9),
        ("map", 10),
        ("int_array", 11),
        ("long_array", 12),
        // A unit enum variant is a String holding the variant's name.
        ("mode", 8),
        ("dynamic", 10),
        ("present", 3),
    ] {
        assert_eq!(
            get(&v, key).discriminant(),
            tag,
            "`{key}` must carry tag {tag}"
        );
    }
    // The two distinctions a derived struct cannot express any other way.
    assert!(
        get(&v, "byte_array").is_byte_array(),
        "Vec<u8> is not a List"
    );
    assert!(
        get(&v, "int_array").is_int_array(),
        "Vec<i32> is not a List"
    );
    assert!(get(&v, "long_array").is_long_array());
    assert!(get(&v, "list").is_list(), "Vec<String> is a List");
    assert_eq!(get(&v, "mode"), &Value::String("Creative".into()));
    // `true` is byte 1 and `u8` keeps its bit pattern in a signed Byte tag.
    assert_eq!(get(&v, "flag"), &Value::Byte(1));
    assert_eq!(get(&v, "ubyte"), &Value::Byte(-56));
}

/// The invariant again, one small type at a time, so a failure points at the
/// exact shape that broke rather than at a 20-field struct.
#[cfg(feature = "nbt")]
#[test]
fn every_shape_matches_the_binary_codec_individually() {
    #[derive(Facet, Debug, PartialEq)]
    struct S<T> {
        v: T,
    }

    assert_matches_binary!(S<bool>, S { v: false });
    assert_matches_binary!(S<i8>, S { v: i8::MIN });
    assert_matches_binary!(S<u8>, S { v: u8::MAX });
    assert_matches_binary!(S<i16>, S { v: i16::MIN });
    assert_matches_binary!(S<i32>, S { v: i32::MIN });
    assert_matches_binary!(S<i64>, S { v: i64::MAX });
    assert_matches_binary!(S<f32>, S { v: -0.5 });
    assert_matches_binary!(S<f64>, S { v: f64::MAX });
    assert_matches_binary!(S<String>, S { v: String::new() });
    assert_matches_binary!(S<Vec<u8>>, S { v: vec![] });
    assert_matches_binary!(
        S<Vec<i32>>,
        S {
            v: vec![i32::MIN, 0]
        }
    );
    assert_matches_binary!(S<Vec<i64>>, S { v: vec![] });
    assert_matches_binary!(S<Vec<String>>, S { v: vec![] });
    assert_matches_binary!(
        S<Vec<Vec<i32>>>,
        S {
            v: vec![vec![1], vec![]]
        }
    );
    assert_matches_binary!(S<[i32; 3]>, S { v: [1, 2, 3] });
    assert_matches_binary!(S<[u8; 2]>, S { v: [7, 8] });
    assert_matches_binary!(S<BTreeMap<String, i32>>, S { v: BTreeMap::new() });
    assert_matches_binary!(S<Option<i32>>, S { v: None });
    assert_matches_binary!(S<Option<i32>>, S { v: Some(-1) });
    assert_matches_binary!(S<Mode>, S { v: Mode::Survival });
    assert_matches_binary!(
        S<Inner>,
        S {
            v: Inner {
                id: 0,
                label: "x".into()
            }
        }
    );
}

/// A bare (non-compound) root is just as valid a document as a struct, so the
/// equivalence must hold for scalar and sequence roots too.
#[cfg(feature = "nbt")]
#[test]
fn non_compound_roots_match_the_binary_codec() {
    assert_matches_binary!(i32, 7);
    assert_matches_binary!(String, "root".to_owned());
    assert_matches_binary!(Vec<u8>, vec![1, 2, 3]);
    assert_matches_binary!(Vec<i64>, vec![1, 2, 3]);
    assert_matches_binary!(Mode, Mode::Survival);
    assert_matches_binary!(BTreeMap<String, i32>, [("k".to_owned(), 1)].into_iter().collect());
}

// --- `Value` itself ---------------------------------------------------------

/// `Value` is special-cased by shape id in both directions, exactly as the
/// byte-level codec special-cases it: it must pass through *unchanged* rather
/// than being walked as the two-dozen-variant enum it is. Walking it would turn
/// `Value::Int(1)` into a `String` tag holding `"Int"`, which is the failure
/// this test exists to catch.
#[test]
fn value_passes_through_unchanged_in_both_directions() {
    let tree = compound([
        ("byte", Value::Byte(-1)),
        ("short", Value::Short(300)),
        ("int", Value::Int(70_000)),
        ("long", Value::Long(5_000_000_000)),
        ("float", Value::Float(1.5)),
        ("double", Value::Double(2.25)),
        ("byte_array", Value::ByteArray(vec![0xde, 0xad])),
        ("string", Value::String(BString::from(vec![0xff, b'x']))),
        ("list", Value::List(vec![Value::Int(1), Value::Int(2)])),
        ("compound", compound([("x", Value::Double(0.5))])),
        ("int_array", Value::IntArray(vec![1, 2, 3])),
        ("long_array", Value::LongArray(vec![-1, -2])),
    ]);

    assert_eq!(to_value(&tree).unwrap(), tree, "T = Value must be identity");
    assert_eq!(
        from_value::<Value>(tree.clone()).unwrap(),
        tree,
        "Value -> Value must be identity"
    );
    // And the composition, in both orders.
    assert_eq!(from_value::<Value>(to_value(&tree).unwrap()).unwrap(), tree);
}

/// A `Value`-typed *field* keeps the exact tags of its subtree, which is the
/// whole reason to reach for one: a `Vec<u8>` field could not have held that
/// `LongArray`.
#[test]
fn value_typed_field_keeps_exact_child_tags() {
    #[derive(Facet, Debug, PartialEq)]
    struct S {
        any: Value,
    }
    let s = S {
        any: Value::List(vec![Value::LongArray(vec![1]), Value::LongArray(vec![])]),
    };
    let v = to_value(&s).unwrap();
    assert_eq!(get(&v, "any"), &s.any);
    assert_eq!(from_value::<S>(v).unwrap(), s);
}

/// Reading a whole document into `Value` and re-typing it afterwards is the
/// use case the feature was asked for: decode once, inspect dynamically, then
/// project into a struct without re-encoding.
#[cfg(feature = "nbt")]
#[test]
fn a_decoded_document_can_be_projected_into_a_struct() {
    let bytes = nbtx::to_be_bytes(&sample()).unwrap();
    let dynamic: Value = nbtx::from_be_bytes(&mut bytes.as_slice()).unwrap();
    let typed: AllTags = from_value(dynamic).unwrap();
    assert_eq!(typed, sample());
}

// --- options ----------------------------------------------------------------

/// A `None` field is *omitted* from the compound, and a key that is absent
/// reads back as `None`. Both halves matter: an omitted key must not become
/// `Some(default)`, and a `None` must not become a present null-ish tag.
#[test]
fn absent_option_is_omitted_and_reads_back_as_none() {
    #[derive(Facet, Debug, PartialEq)]
    struct S {
        a: Option<i32>,
        b: Option<String>,
    }

    let v = to_value(&S {
        a: Some(5),
        b: None,
    })
    .unwrap();
    let map = v.as_compound().unwrap();
    assert_eq!(map.len(), 1, "a `None` field must not appear at all");
    assert_eq!(get(&v, "a"), &Value::Int(5));

    assert_eq!(
        from_value::<S>(v).unwrap(),
        S {
            a: Some(5),
            b: None
        }
    );
    // An entirely empty compound fills both options with `None`.
    assert_eq!(
        from_value::<S>(compound([])).unwrap(),
        S { a: None, b: None }
    );
}

// --- BString and non-UTF-8 --------------------------------------------------

/// A `BString` field is an NBT *string* (tag 8), not a `ByteArray` — even
/// though it reflects as a list of bytes — and its raw bytes survive both
/// directions untouched.
#[test]
fn bstring_fields_are_strings_and_keep_non_utf8_bytes() {
    #[derive(Facet, Debug, PartialEq)]
    struct S {
        raw: BString,
    }
    let raw = BString::from(vec![0xff, 0x00, 0xfe, b'a']);
    let s = S { raw: raw.clone() };

    let v = to_value(&s).unwrap();
    assert_eq!(
        get(&v, "raw").discriminant(),
        8,
        "a BString is a String tag, not a ByteArray"
    );
    assert_eq!(get(&v, "raw"), &Value::String(raw.clone()));
    assert_eq!(from_value::<S>(v).unwrap(), s);

    // The same bytes reaching a `String` field must be refused, not mangled:
    // that is what `BString`/`Value` exist for.
    #[derive(Facet, Debug)]
    struct Utf8 {
        raw: String,
    }
    assert!(
        from_value::<Utf8>(compound([("raw", Value::String(raw))])).is_err(),
        "non-UTF-8 bytes must not silently land in a `String` field"
    );
}

/// Compound *keys* are raw bytes too, so a non-UTF-8 key must survive
/// `to_value` and be matched (or rejected) by its bytes, not by a lossy
/// rendering.
#[test]
fn non_utf8_compound_keys_survive() {
    let key = BString::from(vec![0xff, 0xfe]);
    let tree = Value::Compound(Compound::from_iter([(key.clone(), Value::Int(1))]));
    assert_eq!(to_value(&tree).unwrap(), tree);
    assert_eq!(from_value::<Value>(tree.clone()).unwrap(), tree);
}

// --- unknown fields ---------------------------------------------------------

/// Unknown keys are an error by default, so schema drift cannot quietly discard
/// data — the same rule, and the same error, as the byte-level decoder.
#[test]
fn unknown_key_is_rejected_by_default() {
    #[derive(Facet, Debug)]
    struct S {
        known: i32,
    }
    let err = from_value::<S>(compound([
        ("known", Value::Int(1)),
        ("extra", Value::Byte(2)),
    ]))
    .expect_err("an unknown key must be refused");
    match err {
        nbtx::Error::UnknownField(e) => {
            assert_eq!(e.field(), "extra");
            assert_eq!(e.container(), "S");
        }
        other => panic!("expected UnknownField, got {other:?}"),
    }
}

/// `#[facet(nbtx::allow_unknown_fields)]` opts a single struct back into lenient
/// decoding, and the skipped value may be an arbitrarily complex subtree.
#[test]
fn allow_unknown_fields_skips_extra_keys() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(nbtx::allow_unknown_fields)]
    struct S {
        known: i32,
    }
    let s: S = from_value(compound([
        ("known", Value::Int(1)),
        ("extra", Value::Byte(2)),
        (
            "big",
            compound([("nested", Value::List(vec![Value::Int(1)]))]),
        ),
    ]))
    .unwrap();
    assert_eq!(s, S { known: 1 });
}

/// The opt-out is per struct: an inner struct without the attribute still
/// rejects its own unknown keys.
#[test]
fn allow_unknown_fields_does_not_leak_into_nested_structs() {
    #[derive(Facet, Debug)]
    struct Strict {
        a: i32,
    }
    #[derive(Facet, Debug)]
    #[facet(nbtx::allow_unknown_fields)]
    struct Lenient {
        inner: Strict,
    }
    let tree = compound([(
        "inner",
        compound([("a", Value::Int(1)), ("b", Value::Int(2))]),
    )]);
    assert!(
        from_value::<Lenient>(tree).is_err(),
        "the inner struct must still reject its own unknown key"
    );
}

/// A map accepts any key by definition, so unknown-field denial must not apply
/// to it.
#[test]
fn map_targets_accept_any_key() {
    let m: BTreeMap<String, i32> =
        from_value(compound([("x", Value::Int(1)), ("y", Value::Int(2))])).unwrap();
    assert_eq!(m, [("x".to_owned(), 1), ("y".to_owned(), 2)].into());
}

// --- type mismatches --------------------------------------------------------

/// A tag that cannot fill the target field is an `UnexpectedType` naming both
/// sides — never a silent coercion. Covers every scalar plus the container
/// tags, in both directions of mismatch.
#[test]
fn tag_mismatch_reports_expected_and_found() {
    macro_rules! case {
        ($ty:ty, $value:expr, $expected:expr, $found:expr) => {{
            let err = from_value::<$ty>($value).expect_err(concat!(
                "a mismatched tag must be refused for ",
                stringify!($ty)
            ));
            match err {
                nbtx::Error::UnexpectedType(e) => {
                    assert_eq!(e.expected(), $expected, "wrong `expected` tag");
                    assert_eq!(e.found(), $found, "wrong `found` tag");
                }
                other => panic!("expected UnexpectedType, got {other:?}"),
            }
        }};
    }
    use nbtx::FieldType::{
        Byte, Compound as Comp, Double, Float, Int, List, Long, Short, String as Str,
    };

    // A `List` reaching a target that expects a scalar.
    case!(i32, Value::List(vec![Value::Int(1)]), Int, List);
    case!(bool, Value::List(vec![]), Byte, List);
    case!(String, Value::List(vec![]), Str, List);
    // Scalar-for-scalar mismatches: NBT's integer widths are distinct types.
    case!(i32, Value::Long(1), Int, Long);
    case!(i64, Value::Int(1), Long, Int);
    case!(i16, Value::Byte(1), Short, Byte);
    case!(f64, Value::Float(1.0), Double, Float);
    case!(String, Value::Int(1), Str, Int);
    case!(BString, Value::Int(1), Str, Int);
    case!(Mode, Value::Int(1), Str, Int);
    // Containers.
    case!(Vec<i32>, Value::Int(1), List, Int);
    case!(BTreeMap<String, i32>, Value::Int(1), Comp, Int);
    case!(Inner, Value::Int(1), Comp, Int);
    case!(Inner, Value::List(vec![]), Comp, List);
    // ... and a mismatch on a nested field, not just at the root.
    case!(Inner, compound([("id", Value::Byte(1))]), Int, Byte);
}

/// The three typed arrays are accepted wherever a sequence is expected, exactly
/// as the byte-level reader accepts them: the container tag fixes the element
/// tag, and the target type still decides how each element is stored.
#[test]
fn typed_arrays_fill_any_matching_sequence_target() {
    assert_eq!(
        from_value::<Vec<i8>>(Value::ByteArray(vec![0xff, 0x01])).unwrap(),
        vec![-1i8, 1]
    );
    assert_eq!(
        from_value::<Vec<i32>>(Value::IntArray(vec![1, 2])).unwrap(),
        vec![1, 2]
    );
    assert_eq!(
        from_value::<Vec<u8>>(Value::List(vec![Value::Byte(-1)])).unwrap(),
        vec![255u8]
    );
    // A fixed-size array must match the length exactly.
    assert_eq!(
        from_value::<[i32; 2]>(Value::IntArray(vec![1, 2])).unwrap(),
        [1, 2]
    );
    assert!(
        from_value::<[i32; 3]>(Value::IntArray(vec![1, 2])).is_err(),
        "a length mismatch must be refused, not zero-filled"
    );
}

/// `TAG_Byte` is a signed integer that happens to be used as a flag, so only
/// `1` is `true` — a `!= 0` test would disagree with the Bedrock decoders that
/// wrote the data. This mirrors `nbt::de::read_scalar`.
#[test]
fn only_byte_one_is_true() {
    for (byte, expected) in [(0i8, false), (1, true), (2, false), (-1, false)] {
        assert_eq!(
            from_value::<bool>(Value::Byte(byte)).unwrap(),
            expected,
            "byte {byte} must decode to {expected}"
        );
    }
    assert_eq!(to_value(&true).unwrap(), Value::Byte(1));
    assert_eq!(to_value(&false).unwrap(), Value::Byte(0));
}

/// An enum variant that does not exist is an error, not a silent default.
#[test]
fn unknown_enum_variant_is_rejected() {
    assert!(from_value::<Mode>(Value::String("Hardcore".into())).is_err());
}

/// A required field with no matching key is an error from `Partial::build`,
/// not a zero value.
#[test]
fn missing_required_field_is_rejected() {
    assert!(
        from_value::<Inner>(compound([("id", Value::Int(1))])).is_err(),
        "`label` is not optional and has no default"
    );
}

// --- nested structures ------------------------------------------------------

/// Nested structs, a `Vec` of structs and a map field, checked structurally
/// rather than only through a round-trip.
#[test]
fn nested_structs_lists_and_maps_convert_structurally() {
    let v = to_value(&sample()).unwrap();

    let nested = get(&v, "nested");
    assert_eq!(get(nested, "id"), &Value::Int(7));
    assert_eq!(get(nested, "label"), &Value::String("inner".into()));

    let structs = get(&v, "structs").as_list().expect("a List of Compounds");
    assert_eq!(structs.len(), 2);
    assert_eq!(get(&structs[0], "id"), &Value::Int(1));
    assert_eq!(get(&structs[1], "label"), &Value::String("two".into()));

    let map = get(&v, "map").as_compound().expect("a map is a Compound");
    // A `BTreeMap` iterates sorted, so its key order is fixed either way the
    // `preserve_order` feature is set.
    let keys: Vec<String> = map.keys().map(ToString::to_string).collect();
    assert_eq!(keys, ["a", "b"]);

    assert_eq!(from_value::<AllTags>(v).unwrap(), sample());
}

/// An empty sequence stays a `List` (not a typed array of nothing), and an
/// empty compound stays a compound — the two cases where a tag has to be
/// decided without a sample element.
#[cfg(feature = "nbt")]
#[test]
fn empty_containers_match_the_binary_codec() {
    #[derive(Facet, Debug, PartialEq)]
    struct S {
        list: Vec<String>,
        bytes: Vec<u8>,
        ints: Vec<i32>,
        map: BTreeMap<String, i32>,
        nested: Vec<Vec<i32>>,
    }
    let s = S {
        list: vec![],
        bytes: vec![],
        ints: vec![],
        map: BTreeMap::new(),
        nested: vec![],
    };
    let v = to_value(&s).unwrap();
    assert_eq!(v, via_bytes(&s));
    assert_eq!(get(&v, "list"), &Value::List(vec![]));
    assert_eq!(get(&v, "bytes"), &Value::ByteArray(vec![]));
    assert_eq!(get(&v, "ints"), &Value::IntArray(vec![]));
    assert_eq!(get(&v, "map"), &compound([]));
    assert_eq!(from_value::<S>(v).unwrap(), s);
}

/// `#[facet(rename = "...")]` decides the compound key in both directions, just
/// as it does for the binary codec.
#[cfg(feature = "nbt")]
#[test]
fn renamed_fields_use_their_effective_name() {
    #[derive(Facet, Debug, PartialEq)]
    struct S {
        #[facet(rename = "Level")]
        level: i32,
    }
    let s = S { level: 3 };
    let v = to_value(&s).unwrap();
    assert_eq!(v, via_bytes(&s));
    assert!(
        v.as_compound()
            .unwrap()
            .contains_key(&BString::from("Level"))
    );
    assert_eq!(from_value::<S>(v).unwrap(), s);
    // The Rust name must not be accepted in its place.
    assert!(from_value::<S>(compound([("level", Value::Int(3))])).is_err());
}

/// [`nbtx::Named`] names a *document* root, and a `Value` tree has no document
/// root to name — so here it is just the ordinary two-field compound its own
/// docs describe. Pinned deliberately: this is one of the two places (the other
/// is `heterogeneous_list_is_accepted_by_to_value_and_from_value` below) where
/// `to_value`/`from_value` and the binary codec describe different things,
/// because the byte-level writer has a root-name slot to put the name in and
/// this conversion does not.
#[test]
fn named_is_an_ordinary_compound_here() {
    let doc = nbtx::Named::new("hello world", Value::Int(1));
    let v = to_value(&doc).unwrap();
    assert_eq!(
        v,
        compound([
            ("name", Value::String("hello world".into())),
            ("value", Value::Int(1)),
        ])
    );
    let back: nbtx::Named<Value> = from_value(v).unwrap();
    assert_eq!(back.name, doc.name);
    assert_eq!(back.value, doc.value);
}

/// The second of the two places `to_value`/`from_value` and the binary codec
/// disagree (see `named_is_an_ordinary_compound_here` for the first, and
/// [`nbtx::to_value`]'s own "Notes" section for both): the wire format stores a
/// single element-type byte for a whole `List`, so `to_bytes` rejects a
/// `Value::List` whose elements do not all share the first element's tag. A
/// `Value` tree being built or read directly has no such byte to write or
/// check, so `to_value`/`from_value` have nothing to reject and a heterogeneous
/// list simply passes through unchanged — pinned here rather than left as an
/// assumption, since a passed-through `Value` that merely "doesn't crash" would
/// let this divergence silently widen or narrow later.
#[test]
fn heterogeneous_list_is_accepted_by_to_value_and_from_value() {
    let hetero = compound([("mixed", Value::List(vec![Value::Int(1), Value::Byte(2)]))]);
    assert_eq!(to_value(&hetero).unwrap(), hetero);
    assert_eq!(from_value::<Value>(hetero.clone()).unwrap(), hetero);
}

/// The binary codec really does reject the same value that
/// `heterogeneous_list_is_accepted_by_to_value_and_from_value` accepts, so the
/// two paths are pinned as genuinely disagreeing here rather than merely
/// assumed to.
#[cfg(feature = "nbt")]
#[test]
fn heterogeneous_list_rejected_by_to_bytes_unlike_to_value() {
    let hetero = Value::List(vec![Value::Int(1), Value::Byte(2)]);
    assert!(to_value(&hetero).is_ok(), "to_value must accept it");
    assert!(
        matches!(
            nbtx::to_be_bytes(&hetero),
            Err(nbtx::Error::HeterogeneousList { .. })
        ),
        "to_be_bytes must still reject it"
    );
}

// --- unsupported types ------------------------------------------------------

/// The three shapes with no NBT representation are refused by `to_value` with
/// the same `Unsupported` error the binary writer raises.
#[test]
fn types_without_an_nbt_representation_are_refused() {
    #[derive(Facet)]
    #[facet(nbtx::variant_as(str))]
    #[repr(u8)]
    #[allow(dead_code)]
    enum WithData {
        A(i32),
    }
    assert!(
        matches!(to_value(&WithData::A(1)), Err(nbtx::Error::Unsupported(_))),
        "an enum variant carrying data has no NBT form"
    );

    let map: BTreeMap<i32, i32> = [(1, 2)].into_iter().collect();
    assert!(
        matches!(to_value(&map), Err(nbtx::Error::Unsupported(_))),
        "NBT compound keys are strings"
    );

    let none: Option<i32> = None;
    assert!(
        matches!(to_value(&none), Err(nbtx::Error::Unsupported(_))),
        "a bare `None` is not a document"
    );
}

// --- depth limit ------------------------------------------------------------

/// Builds `depth` nested single-element `Value::List`s around an `Int`.
fn nested_lists(depth: usize) -> Value {
    let mut v = Value::Int(1);
    for _ in 0..depth {
        v = Value::List(vec![v]);
    }
    v
}

/// `to_value` copies a `Value` wholesale instead of walking it node by node, so
/// without an explicit check its nesting would escape the bound every other
/// path is held to — and `to_be_bytes` *does* reject such a value. The two must
/// agree on which values are convertible at all.
#[test]
fn to_value_rejects_a_deeply_nested_value() {
    let deep = nested_lists(2000);
    assert!(
        matches!(to_value(&deep), Err(nbtx::Error::MaxDepthExceeded(_))),
        "2000 nested lists must be refused"
    );
}

#[test]
fn from_value_rejects_a_deeply_nested_value() {
    let deep = nested_lists(2000);
    assert!(
        matches!(
            from_value::<Value>(deep),
            Err(nbtx::Error::MaxDepthExceeded(_))
        ),
        "2000 nested lists must be refused"
    );
}

/// The bound is exactly `MAX_DEPTH` containers in both directions — one deeper
/// fails, one shallower does not. Guards against the limit drifting either way,
/// and against this path enforcing a *different* limit from the binary codec's.
#[test]
fn depth_limit_boundary_is_exactly_max_depth() {
    let at_limit = nested_lists(nbtx::MAX_DEPTH);
    let past_limit = nested_lists(nbtx::MAX_DEPTH + 1);

    assert!(
        to_value(&at_limit).is_ok(),
        "a value exactly at MAX_DEPTH must still convert"
    );
    assert!(
        to_value(&past_limit).is_err(),
        "one container past MAX_DEPTH must be refused"
    );
    assert!(from_value::<Value>(at_limit).is_ok());
    assert!(from_value::<Value>(past_limit).is_err());
}

/// The same bound, reached through the *reflection* path rather than the
/// `Value` fast path: the guard has to live in the struct/sequence walkers too.
///
/// Runs on an explicitly-sized thread for the reason `security_limits.rs`
/// documents: an unoptimised build gives every temporary in a function its own
/// never-reused stack slot, and the reflection path costs several KB per
/// container, which the 2 MiB libtest default would not survive to `MAX_DEPTH`.
#[test]
fn deep_nesting_is_rejected_for_derived_structs() {
    #[derive(Facet, Debug)]
    struct Nest {
        a: Vec<Nest>,
    }

    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            // Each level is `{ "a": List[ <next level> ] }`, i.e. two containers.
            let mut nest = Nest { a: vec![] };
            let mut tree = compound([("a", Value::List(vec![]))]);
            for _ in 0..600 {
                nest = Nest { a: vec![nest] };
                tree = compound([("a", Value::List(vec![tree]))]);
            }

            assert!(
                matches!(to_value(&nest), Err(nbtx::Error::MaxDepthExceeded(_))),
                "a 600-level derived struct must error, not overflow the stack"
            );
            assert!(
                matches!(
                    from_value::<Nest>(tree),
                    Err(nbtx::Error::MaxDepthExceeded(_))
                ),
                "a 600-level compound must error, not overflow the stack"
            );
        })
        .unwrap()
        .join()
        .unwrap();
}

/// The depth guard must agree with the binary codec exactly, not merely exist:
/// a value either converts *and* encodes, or is refused by both.
#[cfg(feature = "nbt")]
#[test]
fn depth_rejection_agrees_with_the_binary_codec() {
    for depth in [1, nbtx::MAX_DEPTH - 1, nbtx::MAX_DEPTH, nbtx::MAX_DEPTH + 1] {
        let v = nested_lists(depth);
        assert_eq!(
            to_value(&v).is_ok(),
            nbtx::to_be_bytes(&v).is_ok(),
            "to_value and to_be_bytes must agree at depth {depth}"
        );
    }
}
