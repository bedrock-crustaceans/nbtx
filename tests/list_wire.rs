//! Byte-exact wire-format vectors for `TAG_List` (tag 9), pinned against two
//! independent NBT implementations:
//!
//! * pmmp/NBT (PHP) — `src/tag/ListTag.php` (big/little-endian only, fixed
//!   4-byte signed length prefix, no varint serializer in this checkout).
//! * gophertunnel (Go) — `minecraft/nbt/decode.go`/`encode.go` (adds the
//!   Bedrock-network `NetworkLittleEndian` variant, whose list length and typed
//!   array lengths are **zigzag varints**, the same encoding `TAG_Int`'s own
//!   payload uses; string lengths are **unsigned** varints instead).
//!
//! Both agree that a list is one element-type byte, then a length, then N bare
//! payloads with no per-element tag or name — the disagreements between them
//! (catalogued in the migration notes this file was written from) are about
//! what happens at the *edges*: empty lists, `TAG_End` as an element type,
//! negative/huge lengths, and out-of-range element types. Each of those gets
//! its own section below, plus the depth accounting a chain of nested lists is
//! cheapest way to exhaust the recursive decoder's stack.
//!
//! `tests/wire_format.rs` and `tests/malformed_input.rs` already cover some of
//! this ground for `List<Short/String/Compound>`; this file adds the
//! array-of-array element types (`List<ByteArray/IntArray/LongArray>`,
//! `List<List<Int>>`) and pins the edge cases explicitly rather than in
//! passing.

#![cfg(feature = "nbt")]

use bstr::BString;
use facet::Facet;
use nbtx::{
    Compound, Error, FieldType, Value, ValueList, from_be_bytes, from_le_bytes, from_varint_bytes,
    to_be_bytes, to_le_bytes, to_varint_bytes,
};

fn hex(s: &str) -> Vec<u8> {
    let packed: String = s.split_whitespace().collect();
    assert!(packed.len().is_multiple_of(2), "odd-length hex: {s}");
    (0..packed.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&packed[i..i + 2], 16).unwrap())
        .collect()
}

fn map(entries: &[(&str, Value)]) -> Compound {
    entries
        .iter()
        .map(|(k, v)| (BString::from(*k), v.clone()))
        .collect()
}

fn get<'a>(v: &'a Value, key: &str) -> &'a Value {
    v.as_compound().unwrap().get(&BString::from(key)).unwrap()
}

/// Asserts `value` encodes to exactly `be`/`le`/`var` in the three variants, and
/// that each of those byte strings decodes back to `value`. Mirrors the helper
/// in `tests/wire_format.rs`.
#[track_caller]
fn assert_vector(label: &str, value: &Value, be: &str, le: &str, var: &str) {
    let cases: [(&str, Vec<u8>, Vec<u8>); 3] = [
        ("BigEndian", to_be_bytes(value).unwrap(), hex(be)),
        ("LittleEndian", to_le_bytes(value).unwrap(), hex(le)),
        ("VarintEndian", to_varint_bytes(value).unwrap(), hex(var)),
    ];
    for (variant, got, want) in cases {
        assert_eq!(
            got,
            want,
            "\n{label} / {variant}: encode mismatch\n  got  = {}\n  want = {}",
            got.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            want.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        );
    }

    let rt_be: Value = from_be_bytes(&mut hex(be).as_slice()).unwrap();
    let rt_le: Value = from_le_bytes(&mut hex(le).as_slice()).unwrap();
    let rt_var: Value = from_varint_bytes(&mut hex(var).as_slice()).unwrap();
    for (variant, rt) in [
        ("BigEndian", rt_be),
        ("LittleEndian", rt_le),
        ("VarintEndian", rt_var),
    ] {
        assert_eq!(&rt, value, "{label} / {variant}: decode mismatch");
    }
}

// ===========================================================================
// 1. Hand-assembled byte vectors for every element type family.
//
// The varint variant's list-length prefix is a **zigzag** varint (so length 1
// is byte 0x02, length 2 is 0x04), exactly like an `Int` payload — gophertunnel
// `encoding.go` routes `WriteInt32`/`Int32` through the same zigzag path for
// both. A string's own length prefix, by contrast, is an **unsigned** varint
// (gophertunnel `String`/`WriteString`), since a length can never be negative.
// ===========================================================================

/// `List<Short>`, two elements, one negative — pins the Short payload as plain
/// little-endian even in the varint variant (only Int/Long/lengths zigzag).
#[test]
fn list_of_shorts_hand_assembled() {
    assert_vector(
        "List<Short>[1,-1]",
        &Value::List(ValueList::Short(vec![1, -1])),
        "09 00 00 02 00 00 00 02 00 01 ff ff",
        "09 00 00 02 02 00 00 00 01 00 ff ff",
        "09 00 02 04 01 00 ff ff", // elem=02, len zigzag(2)=04
    );
}

/// `List<String>`, three elements including an empty string — the string's own
/// length prefix stays unsigned even where the list's length prefix zigzags.
#[test]
fn list_of_strings_hand_assembled() {
    assert_vector(
        "List<String>[\"a\",\"\",\"bb\"]",
        &Value::List(ValueList::String(vec![
            BString::from("a"),
            BString::from(""),
            BString::from("bb"),
        ])),
        "09 00 00 08 00 00 00 03 00 01 61 00 00 00 02 62 62",
        "09 00 00 08 03 00 00 00 01 00 61 00 00 02 00 62 62",
        "09 00 08 06 01 61 00 02 62 62", // list len zigzag(3)=06, string lens unsigned
    );
}

/// `List<ByteArray>`: a non-empty array followed by an empty one. Array length
/// prefixes follow the same rule as the list length itself (zigzag in the
/// varint variant), independently of the outer list's own length.
#[test]
fn list_of_byte_arrays_hand_assembled() {
    assert_vector(
        "List<ByteArray>[[1,2],[]]",
        &Value::List(ValueList::ByteArray(vec![vec![1, 2], vec![]])),
        "09 00 00 07 00 00 00 02 00 00 00 02 01 02 00 00 00 00",
        "09 00 00 07 02 00 00 00 02 00 00 00 01 02 00 00 00 00",
        "09 00 07 04 04 01 02 00", // list len zigzag(2)=04, array len zigzag(2)=04, then zigzag(0)=00
    );
}

/// `List<IntArray>`.
#[test]
fn list_of_int_arrays_hand_assembled() {
    assert_vector(
        "List<IntArray>[[1,-1],[]]",
        &Value::List(ValueList::IntArray(vec![vec![1, -1], vec![]])),
        "09 00 00 0b 00 00 00 02 00 00 00 02 00 00 00 01 ff ff ff ff 00 00 00 00",
        "09 00 00 0b 02 00 00 00 02 00 00 00 01 00 00 00 ff ff ff ff 00 00 00 00",
        "09 00 0b 04 04 02 01 00", // ints zigzag: 1->02, -1->01
    );
}

/// `List<LongArray>`.
#[test]
fn list_of_long_arrays_hand_assembled() {
    assert_vector(
        "List<LongArray>[[1],[]]",
        &Value::List(ValueList::LongArray(vec![vec![1], vec![]])),
        "09 00 00 0c 00 00 00 02 00 00 00 01 00 00 00 00 00 00 00 01 00 00 00 00",
        "09 00 00 0c 02 00 00 00 01 00 00 00 01 00 00 00 00 00 00 00 00 00 00 00",
        "09 00 0c 04 02 02 00", // array len zigzag(1)=02, long zigzag(1)=02, then zigzag(0)=00
    );
}

/// `List<List<Int>>`: the inner lists need not share the outer list's element
/// type story — the second inner list is the untyped empty list (`End`), not an
/// empty `List<Int>`, exactly as pmmp/gophertunnel both allow a list of lists to
/// mix inner element types.
#[test]
fn list_of_lists_of_int_hand_assembled() {
    assert_vector(
        "List<List<Int>>[[1,2],[]]",
        &Value::List(ValueList::List(vec![
            ValueList::Int(vec![1, 2]),
            ValueList::End,
        ])),
        "09 00 00 09 00 00 00 02 03 00 00 00 02 00 00 00 01 00 00 00 02 00 00 00 00 00",
        "09 00 00 09 02 00 00 00 03 02 00 00 00 01 00 00 00 02 00 00 00 00 00 00 00 00",
        "09 00 09 04 03 04 02 04 00 00",
    );
}

/// `List<Compound>`: a non-empty compound followed by an empty one.
#[test]
fn list_of_compounds_hand_assembled() {
    assert_vector(
        "List<Compound>[{a:1b},{}]",
        &Value::List(ValueList::Compound(vec![
            map(&[("a", Value::Byte(1))]),
            map(&[]),
        ])),
        "09 00 00 0a 00 00 00 02 01 00 01 61 01 00 00",
        "09 00 00 0a 02 00 00 00 01 01 00 61 01 00 00",
        "09 00 0a 04 01 01 61 01 00 00",
    );
}

// ===========================================================================
// 2. Empty typed lists: every element type 0..=12, in all three variants.
//
// gophertunnel's `tagByte` fast decode path has an explicit "empty lists are
// allowed to have the TAG_Byte type" carve-out (`decode.go:337-341`) and falls
// through generically for every other declared type at length 0 — i.e. an
// empty list keeps whatever element type it declares, for *every* type, not
// just Byte. pmmp's `ListTag::read()` similarly performs no type validation at
// all when `size == 0`. `ValueList` must therefore round-trip all 13 element
// types (`End` included) at length 0, byte-identically.
// ===========================================================================

fn all_field_types() -> [(u8, FieldType); 13] {
    [
        (0, FieldType::End),
        (1, FieldType::Byte),
        (2, FieldType::Short),
        (3, FieldType::Int),
        (4, FieldType::Long),
        (5, FieldType::Float),
        (6, FieldType::Double),
        (7, FieldType::ByteArray),
        (8, FieldType::String),
        (9, FieldType::List),
        (10, FieldType::Compound),
        (11, FieldType::IntArray),
        (12, FieldType::LongArray),
    ]
}

#[test]
fn every_empty_typed_list_round_trips_with_its_element_type_byte() {
    for (byte, ft) in all_field_types() {
        let list = ValueList::empty(ft);
        let value = Value::List(list.clone());

        let be = to_be_bytes(&value).unwrap();
        let le = to_le_bytes(&value).unwrap();
        let var = to_varint_bytes(&value).unwrap();

        assert_eq!(
            be,
            vec![9, 0, 0, byte, 0, 0, 0, 0],
            "elem type {byte}: BE bytes"
        );
        assert_eq!(
            le,
            vec![9, 0, 0, byte, 0, 0, 0, 0],
            "elem type {byte}: LE bytes"
        );
        // Root header: tag(9) + name-length-varuint(0). Then elem byte, then
        // list length zigzag(0) = 0x00 — one byte each.
        assert_eq!(var, vec![9, 0, byte, 0], "elem type {byte}: varint bytes");

        for (variant, rt) in [
            (
                "BigEndian",
                from_be_bytes::<Value>(&mut be.as_slice()).unwrap(),
            ),
            (
                "LittleEndian",
                from_le_bytes::<Value>(&mut le.as_slice()).unwrap(),
            ),
            (
                "VarintEndian",
                from_varint_bytes::<Value>(&mut var.as_slice()).unwrap(),
            ),
        ] {
            assert_eq!(rt, value, "elem type {byte} / {variant}: decode mismatch");
            assert_eq!(
                rt.as_list().unwrap().element_type(),
                ft,
                "elem type {byte} / {variant}: element type lost"
            );
        }
    }
}

/// `ValueList::End` and an empty `ValueList::Byte` write different bytes
/// (element type 0 vs 1) and must not compare equal — collapsing them would
/// hide a real difference on the wire.
#[test]
fn end_and_empty_byte_list_are_neither_equal_nor_byte_identical() {
    assert_ne!(ValueList::End, ValueList::Byte(vec![]));
    let end_bytes = to_be_bytes(&Value::List(ValueList::End)).unwrap();
    let byte_bytes = to_be_bytes(&Value::List(ValueList::Byte(vec![]))).unwrap();
    assert_ne!(end_bytes, byte_bytes);
}

// ===========================================================================
// 3. Non-empty `TAG_End`-typed list is an error, not a panic.
//
// pmmp: `NbtDataException("Unexpected non-empty list of TAG_End")`
// (`ListTag.php:220-223`). gophertunnel has no dedicated check, but the
// generic element decode hits `case tagEnd: return UnexpectedTagError{...}`
// (`decode.go:157-158`) on the first element — both reject it, neither panics.
// ===========================================================================

#[test]
fn non_empty_tag_end_list_is_rejected_as_unexpected_end_in_every_variant() {
    // tag=List(09), empty root name, elem type=End(00), length=1 (non-empty).
    let be = hex("09 00 00 00 00 00 00 01");
    let le = hex("09 00 00 00 01 00 00 00");
    let var = hex("09 00 00 02"); // len zigzag(1) = 02

    for (variant, bytes) in [("BigEndian", &be), ("LittleEndian", &le), ("Varint", &var)] {
        let err = match variant {
            "BigEndian" => from_be_bytes::<Value>(&mut bytes.as_slice()).unwrap_err(),
            "LittleEndian" => from_le_bytes::<Value>(&mut bytes.as_slice()).unwrap_err(),
            _ => from_varint_bytes::<Value>(&mut bytes.as_slice()).unwrap_err(),
        };
        assert!(
            matches!(err, Error::UnexpectedEnd(_)),
            "{variant}: expected UnexpectedEnd, got {err:?}"
        );
    }
}

/// Element byte 0 (`TAG_End`) at length 0 is the *only* legal use of `TAG_End`
/// as an element type, and decodes to `ValueList::End`.
#[test]
fn empty_tag_end_list_decodes_to_end() {
    let be = hex("09 00 00 00 00 00 00 00");
    let v: Value = from_be_bytes(&mut be.as_slice()).unwrap();
    assert_eq!(v, Value::List(ValueList::End));
}

// ===========================================================================
// 4. Negative and huge length prefixes must not panic or hang.
//
// gophertunnel has **no negative-length guard** anywhere in its list/array
// decode path: `reflect.MakeSlice`/`make([]T, n)` with a negative `int(int32)`
// **panics** at runtime rather than returning an `error` (`decode.go:375-379`,
// `333-342`) — a real crash, not a graceful failure. pmmp instead treats a
// negative `readInt()` length as silently empty (`$size > 0` is false). nbtx
// must do neither: negative and huge lengths are refused with `Err`, never a
// panic and never an unbounded allocation (`MAX_PREALLOC` caps the read-ahead
// buffer regardless of the claimed length).
// ===========================================================================

#[test]
fn negative_length_list_of_int_is_rejected_not_panicking() {
    // elem=Int(03), length = 0xFFFFFFFF (-1 as a signed i32).
    let be = hex("09 00 00 03 ff ff ff ff");
    let le = hex("09 00 00 03 ff ff ff ff"); // palindromic bytes: same either order
    // zigzag-decode(1) = -1, so a raw varint byte of 0x01 supplies length -1.
    let var = hex("09 00 03 01");

    assert!(from_be_bytes::<Value>(&mut be.as_slice()).is_err());
    assert!(from_le_bytes::<Value>(&mut le.as_slice()).is_err());
    assert!(from_varint_bytes::<Value>(&mut var.as_slice()).is_err());
}

#[test]
fn negative_length_list_of_end_is_rejected_not_panicking() {
    // elem=End(00), length = 0xFFFFFFFF (-1 as a signed i32).
    let be = hex("09 00 00 00 ff ff ff ff");
    let le = hex("09 00 00 00 ff ff ff ff");
    let var = hex("09 00 00 01");

    assert!(from_be_bytes::<Value>(&mut be.as_slice()).is_err());
    assert!(from_le_bytes::<Value>(&mut le.as_slice()).is_err());
    assert!(from_varint_bytes::<Value>(&mut var.as_slice()).is_err());
}

/// A huge positive length (`i32::MAX`) with a truncated body must fail
/// promptly rather than committing to an `i32::MAX`-sized allocation up front —
/// the same `MAX_PREALLOC` discipline `tests/security_limits.rs` pins for
/// strings and arrays, exercised here for a list.
#[test]
fn huge_positive_length_with_truncated_body_errs_quickly() {
    let be = hex("09 00 00 03 7f ff ff ff"); // elem=Int, len=i32::MAX, no payload follows
    let le = hex("09 00 00 03 ff ff ff 7f");
    let var = hex("09 00 03 fe ff ff ff 0f"); // zigzag(i32::MAX) = 0xFFFFFFFE

    assert!(from_be_bytes::<Value>(&mut be.as_slice()).is_err());
    assert!(from_le_bytes::<Value>(&mut le.as_slice()).is_err());
    assert!(from_varint_bytes::<Value>(&mut var.as_slice()).is_err());
}

// ===========================================================================
// 5. Out-of-range element type byte.
// ===========================================================================

#[test]
fn out_of_range_element_type_is_type_out_of_range_in_every_variant() {
    for bad in [13u8, 255u8] {
        for len in [0u32, 1u32] {
            let mut be = vec![9u8, 0, 0, bad];
            be.extend_from_slice(&len.to_be_bytes());
            let mut le = vec![9u8, 0, 0, bad];
            le.extend_from_slice(&len.to_le_bytes());
            let mut var = vec![9u8, 0, bad];
            // zigzag(0) = 0x00, zigzag(1) = 0x02 — `len` is only ever 0 or 1 here.
            var.push(if len == 0 { 0x00 } else { 0x02 });

            let be_err = from_be_bytes::<Value>(&mut be.as_slice()).unwrap_err();
            let le_err = from_le_bytes::<Value>(&mut le.as_slice()).unwrap_err();
            let var_err = from_varint_bytes::<Value>(&mut var.as_slice()).unwrap_err();
            for (variant, err) in [("BE", be_err), ("LE", le_err), ("Varint", var_err)] {
                assert!(
                    matches!(err, Error::TypeOutOfRange(_)),
                    "elem={bad} len={len} {variant}: expected TypeOutOfRange, got {err:?}"
                );
            }
        }
    }
}

// ===========================================================================
// 6. Depth: a chain of nested lists exactly `MAX_DEPTH` deep.
//
// Convention pinned by `tests/security_limits.rs::depth_limit_boundary_is_exactly_max_depth`:
// `check_depth(depth)` runs once per container *before* it is entered, and a
// root value is entered at `depth = 0`, so a document with exactly `MAX_DEPTH`
// nested containers still decodes and one more is refused. Here the containers
// are the lists/compounds themselves (no wrapping root compound needed, unlike
// `security_limits.rs`, since the root value here already *is* a `List`).
// ===========================================================================

/// A chain of `n` nested single-element `List`s: `n - 1` `List<List<..>>`
/// wrappers around one innermost `List<Int>` leaf. `n` is the total number of
/// `List` containers entered — the innermost `List<Int>` counts as one of
/// them, since it is itself a container with its own element-type-and-length
/// header, distinct from the `n - 1` `List`-of-`List` wrappers around it.
fn nested_int_list(n: usize) -> ValueList {
    let mut v = ValueList::Int(vec![42]);
    for _ in 1..n {
        v = ValueList::List(vec![v]);
    }
    v
}

/// Hand-assembled bytes for [`nested_int_list`], built independently of the
/// encoder so the read-side boundary does not depend on the write side being
/// correct.
fn nested_int_list_bytes(n: usize) -> Vec<u8> {
    let mut buf = vec![9u8, 0, 0]; // root: tag=List, empty name
    for _ in 1..n {
        buf.push(9); // element type List
        buf.extend_from_slice(&1i32.to_be_bytes()); // length 1
    }
    buf.push(3); // innermost element type Int
    buf.extend_from_slice(&1i32.to_be_bytes()); // length 1
    buf.extend_from_slice(&42i32.to_be_bytes()); // payload
    buf
}

/// A chain of `n` containers alternating `List<Compound>` and `Compound{a: ..}`,
/// starting with `List` at the outermost level — the mixed-container shape
/// `tests/security_limits.rs::deep_nesting_is_rejected_for_derived_structs`
/// exercises for the derive path, pinned here for the dynamic `Value` path.
fn alt_list_compound_chain(n: usize) -> Value {
    fn build(remaining: usize, is_list: bool) -> Value {
        if remaining == 0 {
            return Value::Byte(42);
        }
        if is_list {
            Value::List(ValueList::try_from(vec![build(remaining - 1, false)]).unwrap())
        } else {
            Value::Compound(map(&[("a", build(remaining - 1, true))]))
        }
    }
    build(n, true)
}

#[test]
fn nested_list_depth_boundary_matches_max_depth_on_write_and_read() {
    let at_limit = Value::List(nested_int_list(nbtx::MAX_DEPTH));
    let over_limit = Value::List(nested_int_list(nbtx::MAX_DEPTH + 1));

    // Write side: exactly MAX_DEPTH containers still encodes, byte-identically
    // to the hand-built vector; one container further is refused.
    let bytes = to_be_bytes(&at_limit).expect("MAX_DEPTH nested lists must still encode");
    assert_eq!(bytes, nested_int_list_bytes(nbtx::MAX_DEPTH));
    assert!(
        matches!(to_be_bytes(&over_limit), Err(Error::MaxDepthExceeded(_))),
        "one list past MAX_DEPTH must be refused on write"
    );

    // Read side, built independently of the encoder.
    let at_limit_bytes = nested_int_list_bytes(nbtx::MAX_DEPTH);
    assert_eq!(
        from_be_bytes::<Value>(&mut at_limit_bytes.as_slice()).unwrap(),
        at_limit,
        "a document exactly at MAX_DEPTH must still decode"
    );
    let over_limit_bytes = nested_int_list_bytes(nbtx::MAX_DEPTH + 1);
    assert!(
        matches!(
            from_be_bytes::<Value>(&mut over_limit_bytes.as_slice()),
            Err(Error::MaxDepthExceeded(_))
        ),
        "one container past MAX_DEPTH must be refused on read"
    );
}

#[test]
fn alternating_list_compound_depth_boundary_matches_max_depth() {
    let at_limit = alt_list_compound_chain(nbtx::MAX_DEPTH);
    let over_limit = alt_list_compound_chain(nbtx::MAX_DEPTH + 1);

    let bytes = to_be_bytes(&at_limit)
        .expect("MAX_DEPTH alternating List/Compound containers must still encode");
    let back: Value = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(
        back, at_limit,
        "a document exactly at MAX_DEPTH must round-trip"
    );

    assert!(
        matches!(to_be_bytes(&over_limit), Err(Error::MaxDepthExceeded(_))),
        "one alternating container past MAX_DEPTH must be refused on write"
    );
}

// ===========================================================================
// 7. `ValueList` as a struct field type (plus `Vec<ValueList>`/`Option<ValueList>`),
//    round-tripped through every codec.
// ===========================================================================

#[derive(Facet, Debug, Clone, PartialEq)]
struct WithLists {
    single: ValueList,
    many: Vec<ValueList>,
    optional: Option<ValueList>,
}

#[test]
fn value_list_vec_and_option_fields_round_trip_through_binary_snbt_and_value() {
    let cases = [
        WithLists {
            single: ValueList::Int(vec![1, 2, 3]),
            many: vec![
                ValueList::Byte(vec![1]),
                ValueList::String(vec![BString::from("x")]),
                ValueList::End,
            ],
            optional: Some(ValueList::Compound(vec![map(&[("a", Value::Byte(1))])])),
        },
        WithLists {
            single: ValueList::End,
            many: vec![],
            optional: None,
        },
    ];

    for s in &cases {
        macro_rules! check_binary {
            ($to:ident, $from:ident) => {{
                let bytes = $to(s).unwrap();
                let back: WithLists = $from(&mut bytes.as_slice()).unwrap();
                assert_eq!(&back, s, concat!(stringify!($to), " round-trip"));
            }};
        }
        check_binary!(to_be_bytes, from_be_bytes);
        check_binary!(to_le_bytes, from_le_bytes);
        check_binary!(to_varint_bytes, from_varint_bytes);

        #[cfg(feature = "snbt")]
        {
            let text = nbtx::to_string(s).unwrap();
            let back: WithLists = nbtx::from_string(&text).unwrap();
            assert_eq!(&back, s, "SNBT round-trip");
        }

        let value = nbtx::to_value(s).unwrap();
        let back: WithLists = nbtx::from_value(value).unwrap();
        assert_eq!(&back, s, "to_value/from_value round-trip");
    }
}

/// An *empty* typed list field survives the binary codec and `to_value`/
/// `from_value` byte-for-byte, but SNBT has no syntax for an empty list's
/// element type (`[]` is all there is), so it lossily becomes `ValueList::End`
/// — asserted here explicitly rather than left as an accident of round-tripping.
#[test]
fn empty_typed_list_field_survives_binary_but_snbt_loses_the_element_type() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    struct S {
        items: ValueList,
    }
    let s = S {
        items: ValueList::Short(vec![]),
    };

    let bytes = to_be_bytes(&s).unwrap();
    let back: S = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(back.items, ValueList::Short(vec![]));
    assert_eq!(back.items.element_type(), FieldType::Short);

    let value = nbtx::to_value(&s).unwrap();
    let back: S = nbtx::from_value(value).unwrap();
    assert_eq!(back.items, ValueList::Short(vec![]));

    #[cfg(feature = "snbt")]
    {
        let text = nbtx::to_string(&s).unwrap();
        assert_eq!(text, "{items:[]}");
        let back: S = nbtx::from_string(&text).unwrap();
        assert_eq!(
            back.items,
            ValueList::End,
            "SNBT cannot carry an empty list's element type, so it must come back as End"
        );
        assert_ne!(
            back.items, s.items,
            "the SNBT round-trip is lossy for an empty typed list; this must not silently pass"
        );
    }
}

// ===========================================================================
// 8. A `Vec<Value>` field: rejected when mixed, typed-list when uniform.
// ===========================================================================

#[derive(Facet, Debug, Clone, PartialEq)]
struct WithDynList {
    items: Vec<Value>,
}

#[test]
fn mixed_vec_value_field_is_rejected_by_every_writer() {
    let s = WithDynList {
        items: vec![Value::Byte(1), Value::Short(2)],
    };
    assert!(matches!(
        to_be_bytes(&s),
        Err(Error::HeterogeneousList { .. })
    ));
    assert!(matches!(
        nbtx::to_value(&s),
        Err(Error::HeterogeneousList { .. })
    ));
    #[cfg(feature = "snbt")]
    assert!(matches!(
        nbtx::to_string(&s),
        Err(Error::HeterogeneousList { .. })
    ));
}

#[test]
fn uniform_vec_value_field_round_trips_and_decodes_as_a_typed_list() {
    let s = WithDynList {
        items: vec![Value::Int(1), Value::Int(2)],
    };
    let bytes = to_be_bytes(&s).unwrap();
    let back: WithDynList = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(back, s);

    let decoded: Value = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(
        get(&decoded, "items"),
        &Value::List(ValueList::Int(vec![1, 2]))
    );
}

// ===========================================================================
// 9. SNBT list literals.
// ===========================================================================

#[cfg(feature = "snbt")]
mod snbt_lists {
    use super::*;

    /// A list literal that mixes tags has no NBT encoding at all — vanilla
    /// Minecraft's own SNBT parser rejects the same inputs.
    #[test]
    fn heterogeneous_list_literals_are_rejected() {
        for text in ["{a:[1,2b]}", "{a:[1b,2]}", r#"{a:["a",1]}"#, "{a:[{},[]]}"] {
            let res: Result<Value, _> = nbtx::from_string(text);
            assert!(
                matches!(res, Err(Error::HeterogeneousList { .. })),
                "{text}: expected HeterogeneousList, got {res:?}"
            );
        }
    }

    /// Only the *outer* element type has to agree; the inner lists may differ.
    #[test]
    fn list_of_differently_typed_inner_lists_parses() {
        let v: Value = nbtx::from_string(r#"{a:[[1b],["x"],[]]}"#).unwrap();
        assert_eq!(
            get(&v, "a"),
            &Value::List(ValueList::List(vec![
                ValueList::Byte(vec![1]),
                ValueList::String(vec![BString::from("x")]),
                ValueList::End,
            ]))
        );
    }

    #[test]
    fn list_of_compounds_parses() {
        let v: Value = nbtx::from_string("{a:[{a:1},{}]}").unwrap();
        assert_eq!(
            get(&v, "a"),
            &Value::List(ValueList::Compound(vec![
                map(&[("a", Value::Int(1))]),
                map(&[]),
            ]))
        );
    }

    #[test]
    fn empty_list_literal_parses_as_end() {
        let v: Value = nbtx::from_string("{a:[]}").unwrap();
        assert_eq!(get(&v, "a"), &Value::List(ValueList::End));
    }

    #[test]
    fn list_of_two_empty_lists_parses_as_list_of_end() {
        let v: Value = nbtx::from_string("{a:[[],[]]}").unwrap();
        assert_eq!(
            get(&v, "a"),
            &Value::List(ValueList::List(vec![ValueList::End, ValueList::End]))
        );
    }

    /// Rendering every `ValueList` variant produces the expected literal and
    /// parses back equal. A bare `ValueList` is a legal SNBT root (unlike a
    /// bare `Value::List` root at the top level of a document, `ValueList`
    /// takes the `[..]` node directly with no compound wrapper needed).
    #[test]
    fn every_variant_renders_its_expected_literal_and_round_trips() {
        let cases: Vec<(ValueList, &str)> = vec![
            (ValueList::Byte(vec![1, 2]), "[1b,2b]"),
            (ValueList::Short(vec![1]), "[1s]"),
            (ValueList::Int(vec![1]), "[1]"),
            (ValueList::Long(vec![1]), "[1l]"),
            (ValueList::Float(vec![1.5]), "[1.5f]"),
            (ValueList::Double(vec![2.5]), "[2.5d]"),
            (
                ValueList::String(vec![BString::from("a"), BString::from("b")]),
                r#"["a","b"]"#,
            ),
            (ValueList::ByteArray(vec![vec![1], vec![]]), "[[B;1b],[B;]]"),
            (ValueList::IntArray(vec![vec![1], vec![]]), "[[I;1],[I;]]"),
            (ValueList::LongArray(vec![vec![1]]), "[[L;1l]]"),
            (
                ValueList::List(vec![ValueList::Int(vec![1]), ValueList::Int(vec![2])]),
                "[[1],[2]]",
            ),
            (
                ValueList::Compound(vec![map(&[("a", Value::Int(1))]), map(&[])]),
                "[{a:1},{}]",
            ),
            (ValueList::End, "[]"),
        ];
        for (list, expected) in cases {
            let text = nbtx::to_string(&list).unwrap();
            assert_eq!(text, expected, "{list:?} rendered wrong");
            let back: ValueList = nbtx::from_string(&text).unwrap();
            assert_eq!(back, list, "{expected} did not parse back equal");
        }
    }
}

// ===========================================================================
// 10. `ValueList` iteration helpers, as the codecs use them.
// ===========================================================================

#[test]
fn into_values_yields_the_right_value_tag_for_every_variant() {
    assert_eq!(ValueList::End.into_values(), Vec::<Value>::new());
    assert_eq!(
        ValueList::Byte(vec![1, 2]).into_values(),
        vec![Value::Byte(1), Value::Byte(2)]
    );
    assert_eq!(
        ValueList::Short(vec![1]).into_values(),
        vec![Value::Short(1)]
    );
    assert_eq!(ValueList::Int(vec![1]).into_values(), vec![Value::Int(1)]);
    assert_eq!(ValueList::Long(vec![1]).into_values(), vec![Value::Long(1)]);
    assert_eq!(
        ValueList::Float(vec![1.5]).into_values(),
        vec![Value::Float(1.5)]
    );
    assert_eq!(
        ValueList::Double(vec![1.5]).into_values(),
        vec![Value::Double(1.5)]
    );
    assert_eq!(
        ValueList::ByteArray(vec![vec![1, 2]]).into_values(),
        vec![Value::ByteArray(vec![1, 2])]
    );
    assert_eq!(
        ValueList::String(vec![BString::from("x")]).into_values(),
        vec![Value::String(BString::from("x"))]
    );
    assert_eq!(
        ValueList::List(vec![ValueList::Byte(vec![1])]).into_values(),
        vec![Value::List(ValueList::Byte(vec![1]))]
    );
    assert_eq!(
        ValueList::Compound(vec![map(&[("a", Value::Byte(1))])]).into_values(),
        vec![Value::Compound(map(&[("a", Value::Byte(1))]))]
    );
    assert_eq!(
        ValueList::IntArray(vec![vec![1]]).into_values(),
        vec![Value::IntArray(vec![1])]
    );
    assert_eq!(
        ValueList::LongArray(vec![vec![1]]).into_values(),
        vec![Value::LongArray(vec![1])]
    );
}

#[test]
fn from_value_into_vec_i16_accepts_a_matching_typed_list() {
    let v = Value::List(ValueList::Short(vec![1, 2, 3]));
    let back: Vec<i16> = nbtx::from_value(v).unwrap();
    assert_eq!(back, vec![1, 2, 3]);
}

#[test]
fn from_value_into_vec_vec_u8_accepts_a_list_of_byte_arrays() {
    let v = Value::List(ValueList::ByteArray(vec![vec![1, 2], vec![]]));
    let back: Vec<Vec<u8>> = nbtx::from_value(v).unwrap();
    assert_eq!(back, vec![vec![1, 2], vec![]]);
}

#[test]
fn from_value_into_vec_i8_rejects_a_list_of_int_as_unexpected_type() {
    let v = Value::List(ValueList::Int(vec![1]));
    let err = nbtx::from_value::<Vec<i8>>(v).unwrap_err();
    assert!(
        matches!(err, Error::UnexpectedType(_)),
        "expected UnexpectedType, got {err:?}"
    );
}
