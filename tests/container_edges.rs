//! Edge cases of the two recursive containers, `List` and `Compound`.
//!
//! `tag_semantics.rs` establishes the rules (one element type per list,
//! first-wins duplicate keys, empty-list normalisation). This file works the
//! *shapes* those rules leave open and that nothing else exercises: lists whose
//! elements are themselves containers, arrays nested in lists, keys that are
//! empty or contain NUL, containers wide enough to leave the small-array paths,
//! and the exact point at which a heterogeneous list is detected.

#![cfg(feature = "nbt")]

use bstr::BString;
use nbtx::{
    Compound, Value, ValueList, from_be_bytes, from_le_bytes, from_varint_bytes, to_be_bytes,
    to_le_bytes, to_varint_bytes,
};

macro_rules! for_each_endian {
    ($check:ident) => {
        $check!(to_be_bytes, from_be_bytes);
        $check!(to_le_bytes, from_le_bytes);
        $check!(to_varint_bytes, from_varint_bytes);
    };
}

fn map(entries: &[(&str, Value)]) -> Compound {
    entries
        .iter()
        .map(|(k, v)| (BString::from(*k), v.clone()))
        .collect()
}

fn comp(entries: &[(&str, Value)]) -> Value {
    Value::Compound(map(entries))
}

/// Round-trips `doc` in every variant and asserts it comes back unchanged and
/// re-encodes to the identical bytes.
#[track_caller]
fn roundtrips(label: &str, doc: &Value) {
    macro_rules! check {
        ($to:ident, $from:ident) => {{
            let bytes = $to(doc).unwrap_or_else(|e| panic!("{label} / {}: {e}", stringify!($to)));
            let back: Value = $from(&mut bytes.as_slice())
                .unwrap_or_else(|e| panic!("{label} / {}: {e}", stringify!($from)));
            assert_eq!(&back, doc, "{label} / {}", stringify!($to));
            assert_eq!($to(&back).unwrap(), bytes, "{label}: re-encode differed");
        }};
    }
    for_each_endian!(check);
}

// --- lists of containers --------------------------------------------------

/// A list whose elements are lists: legal, because every element shares tag 9,
/// even though the *inner* lists have different element types and lengths. The
/// element-type check is per level, not recursive.
#[test]
fn a_list_of_lists_may_hold_differently_typed_inner_lists() {
    let doc = comp(&[(
        "l",
        Value::List(ValueList::List(vec![
            ValueList::Byte(vec![1, 2]),
            ValueList::String(vec![BString::from("x")]),
            ValueList::End,
        ])),
    )]);
    roundtrips("list of lists", &doc);

    // The outer element type byte is TAG_List (9), whatever the inner ones are.
    let bytes = to_be_bytes(&doc).unwrap();
    assert_eq!(bytes[3], 9, "the entry is a TAG_List");
    assert_eq!(bytes[7], 9, "its element type is TAG_List too");
}

/// A list of compounds with *different key sets* is legal for the same reason:
/// all compounds are tag 10, and NBT has no notion of a compound's schema.
#[test]
fn a_list_of_compounds_may_hold_different_key_sets() {
    let doc = comp(&[(
        "l",
        Value::List(ValueList::Compound(vec![
            map(&[("a", Value::Int(1))]),
            map(&[("b", Value::Long(2)), ("c", Value::Byte(3))]),
            map(&[]),
        ])),
    )]);
    roundtrips("list of compounds", &doc);
}

/// A list of the three typed arrays, one list per tag: each array is a single
/// element of the outer list, and its own length prefix must not be confused
/// with the list's.
#[test]
fn lists_of_typed_arrays_roundtrip() {
    for (label, doc) in [
        (
            "byte arrays",
            Value::List(ValueList::ByteArray(vec![vec![1, 2], vec![], vec![0xff]])),
        ),
        (
            "int arrays",
            Value::List(ValueList::IntArray(vec![vec![-1], vec![1, 2, 3]])),
        ),
        (
            "long arrays",
            Value::List(ValueList::LongArray(vec![vec![i64::MIN], vec![]])),
        ),
    ] {
        roundtrips(label, &comp(&[("l", doc)]));
    }
}

/// A list of empty lists still declares TAG_List as its element type, so the
/// empty-list normalisation applies to the *inner* lists only and does not
/// propagate outward.
#[test]
fn a_list_of_empty_lists_keeps_its_own_element_type() {
    let doc = comp(&[(
        "l",
        Value::List(ValueList::List(vec![ValueList::End, ValueList::End])),
    )]);
    roundtrips("list of empty lists", &doc);

    let bytes = to_be_bytes(&doc).unwrap();
    assert_eq!(bytes[7], 9, "the outer element type is still TAG_List");
    // Each inner list declares TAG_End, because it has no element to type it.
    assert_eq!(
        bytes[12], 0,
        "the first inner list's element type is TAG_End"
    );
}

/// Alternating containers — a compound inside a list inside a compound, four
/// levels down — exercise both recursive readers switching between each other
/// rather than each recursing into itself.
#[test]
fn alternating_lists_and_compounds_roundtrip() {
    let doc = comp(&[(
        "a",
        Value::List(ValueList::Compound(vec![map(&[(
            "b",
            Value::List(ValueList::Compound(vec![map(&[(
                "c",
                Value::List(ValueList::Double(vec![1.5])),
            )])])),
        )])])),
    )]);
    roundtrips("alternating containers", &doc);
}

// --- heterogeneity --------------------------------------------------------

/// The heterogeneity check must fire wherever the odd element sits, including at
/// the very end of a long list — a check that only compared the first two
/// elements would pass this.
///
/// The check now lives in `ValueList::try_from` rather than in the encoder: a
/// mixed list cannot be built at all, so there is nothing left for `to_bytes`
/// to reject.
#[test]
fn a_heterogeneous_list_is_caught_wherever_the_odd_element_sits() {
    for bad_at in [1usize, 5, 9] {
        let mut items: Vec<Value> = (0..10).map(Value::Byte).collect();
        items[bad_at] = Value::Int(0);
        let res = ValueList::try_from(items);
        assert!(
            matches!(res, Err(nbtx::Error::HeterogeneousList { .. })),
            "an Int at index {bad_at} must be caught, got {res:?}"
        );
    }
}

/// The check applies to a list at every nesting level, not only one at the
/// root — and it applies as the list is *built*, so a document containing a
/// mixed list can never come into existence to be encoded.
#[test]
fn a_heterogeneous_list_nested_deep_is_still_rejected() {
    // The inner list is the mixed one, two containers down.
    let mut inner = ValueList::Byte(vec![1]);
    assert!(matches!(
        inner.push(Value::Short(2)),
        Err(nbtx::Error::HeterogeneousList { .. })
    ));
    assert!(matches!(
        ValueList::try_from(vec![Value::Byte(1), Value::Short(2)]),
        Err(nbtx::Error::HeterogeneousList { .. })
    ));
    // The refused push left the list untouched, so the document around it is
    // still encodable — the failure is local to the list, not to the document.
    let doc = comp(&[(
        "outer",
        Value::List(ValueList::Compound(vec![map(&[(
            "inner",
            Value::List(inner),
        )])])),
    )]);
    roundtrips("nested list after a refused push", &doc);
}

/// Two *different container* tags in one list are heterogeneous too — a `List`
/// and a `Compound` are as incompatible as a `Byte` and an `Int`, even though
/// both are containers.
#[test]
fn containers_of_different_tags_may_not_share_a_list() {
    let doc = ValueList::try_from(vec![Value::List(ValueList::End), comp(&[])]);
    match doc {
        Err(nbtx::Error::HeterogeneousList { expected, found }) => {
            assert_eq!(expected, nbtx::FieldType::List);
            assert_eq!(found, nbtx::FieldType::Compound);
        }
        other => panic!("expected HeterogeneousList, got {other:?}"),
    }
    // Likewise the three array tags, which look alike but are not.
    let arrays = ValueList::try_from(vec![Value::IntArray(vec![1]), Value::LongArray(vec![1])]);
    assert!(matches!(arrays, Err(nbtx::Error::HeterogeneousList { .. })));
}

/// A one-element list can never be heterogeneous, so every tag must be usable as
/// a singleton — including the ones that only appear as list elements in
/// practice.
#[test]
fn a_single_element_list_is_valid_for_every_tag() {
    for element in [
        Value::Byte(1),
        Value::Short(1),
        Value::Int(1),
        Value::Long(1),
        Value::Float(1.0),
        Value::Double(1.0),
        Value::ByteArray(vec![1]),
        Value::String(BString::from("s")),
        Value::List(ValueList::Int(vec![1])),
        comp(&[("k", Value::Int(1))]),
        Value::IntArray(vec![1]),
        Value::LongArray(vec![1]),
    ] {
        let tag = element.discriminant();
        let list = ValueList::try_from(vec![element]).expect("one element is homogeneous");
        let doc = comp(&[("l", Value::List(list))]);
        roundtrips(&format!("singleton list of tag {tag}"), &doc);
        assert_eq!(
            to_be_bytes(&doc).unwrap()[7],
            tag,
            "the element type byte must be the element's own tag"
        );
    }
}

// --- compound keys --------------------------------------------------------

/// A compound key may be empty, and it may contain a NUL byte: NBT strings are
/// length-prefixed raw bytes, so neither terminates the key. A C-style
/// implementation would truncate at the NUL.
#[test]
fn compound_keys_may_be_empty_or_contain_nul_bytes() {
    let doc = Value::Compound(Compound::from([
        (BString::from(""), Value::Int(1)),
        (BString::from(vec![b'a', 0x00, b'b']), Value::Int(2)),
        (BString::from(vec![0x00]), Value::Int(3)),
    ]));
    roundtrips("awkward keys", &doc);

    let bytes = to_be_bytes(&doc).unwrap();
    let back: Value = from_be_bytes(&mut bytes.as_slice()).unwrap();
    let map = back.as_compound().unwrap();
    assert_eq!(map.len(), 3, "all three keys must be distinct");
    assert_eq!(map.get(&BString::from("")), Some(&Value::Int(1)));
    assert_eq!(
        map.get(&BString::from(vec![b'a', 0x00, b'b'])),
        Some(&Value::Int(2))
    );
}

/// Two keys that differ only after a NUL must stay distinct — the case a
/// truncating implementation would silently merge into one entry.
#[test]
fn keys_differing_only_after_a_nul_stay_distinct() {
    let doc = Value::Compound(Compound::from([
        (BString::from(vec![b'k', 0x00, b'1']), Value::Byte(1)),
        (BString::from(vec![b'k', 0x00, b'2']), Value::Byte(2)),
    ]));
    let bytes = to_le_bytes(&doc).unwrap();
    let back: Value = from_le_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(back.as_compound().unwrap().len(), 2);
    assert_eq!(back, doc);
}

/// Duplicate keys are first-wins; when the two occurrences carry *different
/// tags*, the dropped one must still be fully parsed so the stream stays in
/// sync — that is what distinguishes "drop" from "skip the bytes we guessed".
#[test]
fn a_dropped_duplicate_of_a_different_tag_is_still_fully_consumed() {
    // root; Int "d" = 1; Compound "d" { Long "x" = 9 }; Byte "after" = 7; end
    let mut buf = vec![10u8, 0x00, 0x00];
    buf.extend_from_slice(&[3, 0x00, 0x01, b'd']);
    buf.extend_from_slice(&1i32.to_be_bytes());
    buf.extend_from_slice(&[10, 0x00, 0x01, b'd']);
    buf.extend_from_slice(&[4, 0x00, 0x01, b'x']);
    buf.extend_from_slice(&9i64.to_be_bytes());
    buf.push(0); // end of the duplicate compound
    buf.extend_from_slice(&[1, 0x00, 0x05, b'a', b'f', b't', b'e', b'r', 7]);
    buf.push(0); // end of root

    let back: Value = from_be_bytes(&mut buf.as_slice()).unwrap();
    let map = back.as_compound().unwrap();
    assert_eq!(map.get(&BString::from("d")), Some(&Value::Int(1)));
    assert_eq!(
        map.get(&BString::from("after")),
        Some(&Value::Byte(7)),
        "the key after the dropped duplicate must still decode"
    );
    assert_eq!(map.len(), 2);
}

/// A duplicate key nested inside a list element follows the same rule — the
/// compound reader is the same one at every depth.
#[test]
fn the_first_wins_rule_applies_inside_nested_compounds() {
    // root { l: List<Compound>[ { d: 1, d: 2 } ] }
    let mut buf = vec![10u8, 0x00, 0x00];
    buf.extend_from_slice(&[9, 0x00, 0x01, b'l', 10]);
    buf.extend_from_slice(&1i32.to_be_bytes());
    for n in [1i32, 2] {
        buf.extend_from_slice(&[3, 0x00, 0x01, b'd']);
        buf.extend_from_slice(&n.to_be_bytes());
    }
    buf.push(0); // end of the inner compound
    buf.push(0); // end of root

    let back: Value = from_be_bytes(&mut buf.as_slice()).unwrap();
    let list = back.as_compound().unwrap()[&BString::from("l")]
        .as_list()
        .unwrap();
    let inner = list.get(0).unwrap().into_compound().unwrap();
    assert_eq!(inner.len(), 1);
    assert_eq!(inner.get(&BString::from("d")), Some(&Value::Int(1)));
}

// --- size ------------------------------------------------------------------

/// Containers wide enough to leave the small-input paths: past `MAX_PREALLOC`
/// (4 KiB) the readers stop reserving up front and grow the buffer instead, so
/// this covers the branch that short documents never reach.
#[test]
fn containers_larger_than_the_preallocation_cap_roundtrip() {
    let doc = comp(&[
        (
            "bytes",
            Value::ByteArray((0..10_000).map(|i| (i % 251) as u8).collect()),
        ),
        ("ints", Value::IntArray((0..5_000).collect())),
        (
            "longs",
            Value::LongArray((0..5_000).map(i64::from).collect()),
        ),
        // A `List` rather than a typed array: its elements go through the
        // per-element writer, which has no bulk path to fall back on.
        (
            "list",
            Value::List(ValueList::Short((0..5_000i32).map(|i| i as i16).collect())),
        ),
    ]);
    roundtrips("large containers", &doc);
}

/// A compound with many keys, to exercise the duplicate-detection bookkeeping at
/// a size where an accidental quadratic scan would be noticeable and where the
/// map backing actually has to grow.
#[test]
fn a_compound_with_a_thousand_keys_roundtrips() {
    let mut map = Compound::new();
    for i in 0..1_000 {
        map.insert(BString::from(format!("key{i:04}")), Value::Int(i));
    }
    let doc = Value::Compound(map);
    roundtrips("wide compound", &doc);

    let bytes = to_be_bytes(&doc).unwrap();
    let back: Value = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(back.as_compound().unwrap().len(), 1_000);
}

/// A long string as a *key* rather than a value: keys go through the same string
/// codec, so a key close to `MAX_STRING_LEN` must work exactly like a payload of
/// that length.
#[test]
fn a_key_at_the_string_length_limit_roundtrips() {
    let key = BString::from(vec![b'k'; nbtx::MAX_STRING_LEN]);
    let doc = Value::Compound(Compound::from([(key.clone(), Value::Byte(1))]));
    roundtrips("max-length key", &doc);

    let bytes = to_be_bytes(&doc).unwrap();
    let back: Value = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(back.as_compound().unwrap().get(&key), Some(&Value::Byte(1)));
}

/// A deeply nested but legal document — one below `MAX_DEPTH` — must round-trip
/// rather than being caught by an off-by-one in the guard. `security_limits`
/// pins the rejection side; this pins the acceptance side through a full
/// encode/decode/re-encode cycle.
#[test]
fn a_document_one_container_below_the_depth_limit_roundtrips() {
    // The root compound is container 1, so nest MAX_DEPTH - 2 lists inside it.
    let mut inner = Value::Byte(1);
    for _ in 0..nbtx::MAX_DEPTH - 2 {
        inner = Value::List(ValueList::try_from(vec![inner]).expect("a singleton"));
    }
    let doc = comp(&[("deep", inner)]);

    let bytes = to_be_bytes(&doc).expect("a document below MAX_DEPTH must encode");
    let back: Value = from_be_bytes(&mut bytes.as_slice()).expect("...and decode");
    assert_eq!(back, doc);
    assert_eq!(to_be_bytes(&back).unwrap(), bytes);
}

// --- empty containers ------------------------------------------------------

/// Nested empty compounds: the smallest possible nesting, and the one where a
/// reader that forgot to consume the inner `TAG_End` would resynchronise
/// plausibly instead of failing.
#[test]
fn nested_empty_compounds_roundtrip() {
    let doc = comp(&[("a", comp(&[("b", comp(&[("c", comp(&[]))]))]))]);
    roundtrips("nested empties", &doc);

    let bytes = to_be_bytes(&doc).unwrap();
    // Four compounds (the root plus a/b/c) means exactly four trailing TAG_Ends,
    // immediately after the innermost key — an off-by-one here is the mistake a
    // reader that "resynchronises plausibly" would hide.
    assert_eq!(bytes[bytes.len() - 4..], [0, 0, 0, 0]);
    assert_eq!(bytes[bytes.len() - 5], b'c');
}

/// Every empty container in one document, so the five "length zero" paths are
/// exercised together and cannot be confused for one another.
#[test]
fn all_five_empty_containers_keep_their_tags() {
    let doc = comp(&[
        ("ba", Value::ByteArray(vec![])),
        ("ia", Value::IntArray(vec![])),
        ("la", Value::LongArray(vec![])),
        ("li", Value::List(ValueList::End)),
        ("co", comp(&[])),
    ]);
    roundtrips("empty containers", &doc);

    let bytes = to_be_bytes(&doc).unwrap();
    let back: Value = from_be_bytes(&mut bytes.as_slice()).unwrap();
    let map = back.as_compound().unwrap();
    assert!(map[&BString::from("ba")].is_byte_array());
    assert!(map[&BString::from("ia")].is_int_array());
    assert!(map[&BString::from("la")].is_long_array());
    assert!(map[&BString::from("li")].is_list());
    assert!(map[&BString::from("co")].is_compound());
}

/// An empty list *inside* a list, and an empty compound inside a list, are the
/// two shapes where a container has no elements to type it but is itself an
/// element of a typed sequence.
#[test]
fn empty_containers_nested_inside_a_list_roundtrip() {
    roundtrips(
        "empty list in list",
        &comp(&[("l", Value::List(ValueList::List(vec![ValueList::End])))]),
    );
    roundtrips(
        "empty compound in list",
        &comp(&[("l", Value::List(ValueList::Compound(vec![map(&[])])))]),
    );
}
