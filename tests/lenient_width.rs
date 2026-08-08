//! `#[facet(nbtx::lenient_width(<types>))]`: reading a field that arrives in a
//! width other than its own.
//!
//! Real documents drift. The same key is a `Byte` in one version of a game and
//! an `Int` in the next, and the item-rotation field that motivated this
//! attribute is a `Byte` on disk but an `f32` in every sane Rust model of it.
//! The attribute names the *other* tags a field tolerates on decode.
//!
//! Four properties carry this file:
//!
//! * **Additive, never destructive.** The field's own tag is always accepted,
//!   listed or not; a tag that is neither is still an
//!   [`Error::UnexpectedType`], unchanged.
//! * **Lossless or nothing.** A tag the list names is converted only if the
//!   value survives exactly. Anything else is
//!   [`Error::LenientWidthOutOfRange`] — a *different* failure from "wrong tag
//!   entirely", which is the whole point of having two variants.
//! * **Decode only.** Encoding never consults the list; a leniently decoded
//!   value is re-encoded with the field's own tag. The attribute normalises,
//!   it does not preserve.
//! * **Misplacement is loud.** There is no meaning for "widen" on a compound,
//!   a string or a `bool`, so those are [`Error::InvalidLenientWidth`] rather
//!   than being quietly ignored.
//!
//! The `Value` conversion tests need no features; the byte- and text-level ones
//! are gated on `nbt`/`snbt`.

use facet::Facet;
use nbtx::{Compound, Error, Value, from_value, to_value};

// --- the types under test ---------------------------------------------------

/// The issue's own motivating case: a rotation modelled as an `f32` that older
/// writers emit as a `Byte`.
#[derive(Facet, Debug, PartialEq)]
struct ItemRotation {
    #[facet(nbtx::lenient_width(i8))]
    rotation: f32,
}

/// A count modelled as an `i32` that older writers emit as a `Byte`.
#[derive(Facet, Debug, PartialEq)]
struct Count {
    #[facet(nbtx::lenient_width(i8))]
    count: i32,
}

/// The narrowing direction: a `Byte` field that some writers widened to an
/// `Int`. Values outside `i8` must be refused, not truncated.
#[derive(Facet, Debug, PartialEq)]
struct Narrow {
    #[facet(nbtx::lenient_width(i32))]
    level: i8,
}

/// Several accepted tags at once, and both float directions.
#[derive(Facet, Debug, PartialEq)]
struct Wide {
    #[facet(nbtx::lenient_width(i8, i16, i64, f32, f64))]
    value: f32,
}

/// A float arriving where an integer is declared.
#[derive(Facet, Debug, PartialEq)]
struct FromFloat {
    #[facet(nbtx::lenient_width(f32, f64))]
    ticks: i32,
}

/// One declaration, applied to every element / to the inside of the `Option`.
#[derive(Facet, Debug, PartialEq)]
struct Seqs {
    #[facet(nbtx::lenient_width(i8))]
    ids: Vec<i32>,
    #[facet(nbtx::lenient_width(i8))]
    fixed: [i32; 2],
    #[facet(nbtx::lenient_width(i8))]
    maybe: Option<i32>,
}

/// The narrowing direction, element-wise.
#[derive(Facet, Debug, PartialEq)]
struct Narrows {
    #[facet(nbtx::lenient_width(i32))]
    levels: Vec<i8>,
}

#[derive(Facet, Debug, PartialEq)]
struct Inner {
    a: i32,
}

/// Illegal: there is no scalar here to widen.
#[derive(Facet, Debug, PartialEq)]
struct BadCompound {
    #[facet(nbtx::lenient_width(i8))]
    inner: Inner,
}

/// Illegal: `Vec<Struct>` has no scalar leaf either.
#[derive(Facet, Debug, PartialEq)]
struct BadVecOfStruct {
    #[facet(nbtx::lenient_width(i8))]
    inners: Vec<Inner>,
}

/// Illegal: `bool` is not one of the six NBT scalar wire types, and its `Byte`
/// truthiness rule is deliberately untouched by this attribute.
#[derive(Facet, Debug, PartialEq)]
struct BadBool {
    #[facet(nbtx::lenient_width(i8))]
    flag: bool,
}

/// Illegal: a string is not a number.
#[derive(Facet, Debug, PartialEq)]
struct BadString {
    #[facet(nbtx::lenient_width(i8))]
    name: String,
}

/// The discriminant is normally an `Int`; a `Byte` is accepted too.
#[derive(Facet, Debug, PartialEq)]
#[facet(nbtx::variant_as(i32), nbtx::lenient_width(i8))]
#[repr(u8)]
enum Difficulty {
    Peaceful = 0,
    Hard = 3,
}

/// The narrowing direction for a discriminant: declared as a `Byte`, tolerant
/// of an `Int` that fits.
#[derive(Facet, Debug, PartialEq)]
#[facet(nbtx::variant_as(i8), nbtx::lenient_width(i32))]
#[repr(u8)]
enum Slot {
    First = 1,
    Second = 2,
}

/// Illegal: a `str` mode carries variant *names*, which have no width.
#[derive(Facet, Debug, PartialEq)]
#[facet(nbtx::variant_as(str), nbtx::lenient_width(i8))]
#[repr(u8)]
enum BadStrEnum {
    Only,
}

#[derive(Facet, Debug, PartialEq)]
struct Level {
    difficulty: Difficulty,
}

/// An `f64` target: the exact-representability boundary is 2^53, not `Wide`'s
/// 2^24, because `f64` has a 53-bit significand.
#[derive(Facet, Debug, PartialEq)]
struct WideF64 {
    #[facet(nbtx::lenient_width(i8, i16, i32, i64))]
    value: f64,
}

/// `i32` widening into `f32` specifically — the real risk zone the issue calls
/// out, since most `i32` values are *not* exactly representable in `f32`
/// (unlike `Wide`'s `i64`/`f64` sources, which share the same conversion code
/// but not necessarily the same test coverage).
#[derive(Facet, Debug, PartialEq)]
struct FromI32 {
    #[facet(nbtx::lenient_width(i32))]
    value: f32,
}

/// Illegal: a dynamic `Value` already handles every tag itself, so there is
/// nothing left for `lenient_width` to widen.
#[derive(Facet, Debug, PartialEq)]
struct BadValue {
    #[facet(nbtx::lenient_width(i8))]
    v: Value,
}

/// Illegal: `Option<Struct>` has no scalar leaf either, same as `Vec<Struct>`.
#[derive(Facet, Debug, PartialEq)]
struct BadOptionStruct {
    #[facet(nbtx::lenient_width(i8))]
    inner: Option<Inner>,
}

/// Illegal: a bare `u8` is outside the six NBT scalar wire types the attribute
/// may name — `i8` already stands for the `Byte` tag both share.
#[derive(Facet, Debug, PartialEq)]
struct BadU8 {
    #[facet(nbtx::lenient_width(i16))]
    v: u8,
}

/// Illegal: a `Vec<u8>` (the `ByteArray` tag) has no scalar leaf — its element
/// is `u8`, which is not one of the six widenable types either.
#[derive(Facet, Debug, PartialEq)]
struct BadByteArray {
    #[facet(nbtx::lenient_width(i32))]
    data: Vec<u8>,
}

/// Two independent declarations on the same struct, to confirm neither field's
/// list leaks into the other's.
#[derive(Facet, Debug, PartialEq)]
struct MultiLenient {
    #[facet(nbtx::lenient_width(i8))]
    a: i32,
    #[facet(nbtx::lenient_width(f64))]
    b: f32,
}

/// One declaration reaching a scalar leaf through *two* nested containers at
/// once: the `Vec` and the `Option` inside it.
#[derive(Facet, Debug, PartialEq)]
struct NestedOpt {
    #[facet(nbtx::lenient_width(i8))]
    values: Vec<Option<f32>>,
}

/// An unsigned mode wider than a byte's positive range, to pin the
/// sign/bit-pattern rule a widened discriminant follows (see
/// `VariantAs::narrow`/`widen` in `src/reflect.rs`).
#[derive(Facet, Debug, PartialEq)]
#[facet(nbtx::variant_as(u8), nbtx::lenient_width(i16, i32))]
#[repr(u8)]
enum U8Mode {
    Zero = 0,
    TwoHundred = 200,
}

/// `lenient_width(f32)` alone: an untagged decimal literal is always a
/// `Double` by SNBT's own grammar, so only `f32`'s explicit-suffix form
/// (`3.0f`) reaches this field, never a bare `3.0`.
#[derive(Facet, Debug, PartialEq)]
struct SnbtOnlyF32 {
    #[facet(nbtx::lenient_width(f32))]
    ticks: i32,
}

/// The mirror image: `lenient_width(f64)` alone accepts a bare `3.0` but not
/// an explicitly `f32`-suffixed `3.0f`.
#[derive(Facet, Debug, PartialEq)]
struct SnbtOnlyF64 {
    #[facet(nbtx::lenient_width(f64))]
    ticks: i32,
}

// --- helpers ----------------------------------------------------------------

fn comp(entries: &[(&str, Value)]) -> Value {
    Value::Compound(
        entries
            .iter()
            .map(|(k, v)| (bstr::BString::from(*k), v.clone()))
            .collect::<Compound>(),
    )
}

/// Decodes `doc` into `T` through the `Value` conversion *and*, when the binary
/// codec is compiled in, through actual big-endian bytes — the two must agree,
/// because they share the widening logic.
#[track_caller]
fn decode<'f, T: Facet<'f> + PartialEq + std::fmt::Debug>(doc: &Value) -> Result<T, Error> {
    let from_tree = from_value::<T>(doc.clone());

    #[cfg(feature = "nbt")]
    {
        let bytes = nbtx::to_be_bytes(doc).expect("the fixture itself must encode");
        let from_bytes = nbtx::from_be_bytes::<T>(&mut bytes.as_slice());
        match (&from_tree, &from_bytes) {
            (Ok(a), Ok(b)) => assert_eq!(a, b, "from_value and from_be_bytes disagreed"),
            (Err(a), Err(b)) => assert_eq!(
                variant_of(a),
                variant_of(b),
                "from_value and from_be_bytes failed differently: {a} / {b}"
            ),
            (a, b) => panic!("from_value and from_be_bytes disagreed: {a:?} / {b:?}"),
        }
    }

    from_tree
}

/// A coarse discriminator for [`decode`]'s cross-check: only the variants this
/// file can produce need distinguishing.
#[cfg(feature = "nbt")]
fn variant_of(err: &Error) -> &'static str {
    match err {
        Error::UnexpectedType(_) => "UnexpectedType",
        Error::LenientWidthOutOfRange(_) => "LenientWidthOutOfRange",
        Error::InvalidLenientWidth(_) => "InvalidLenientWidth",
        _ => "other",
    }
}

/// Destructures the widening failure into `(value, wire tag, target type)`.
///
/// The error payload structs are `pub` but live in a private module, so they
/// cannot be named from outside the crate — only reached through the `Error`
/// variant, which is exactly how a user would read them.
#[track_caller]
fn out_of_range(err: &Error) -> (String, nbtx::FieldType, &'static str) {
    match err {
        Error::LenientWidthOutOfRange(e) => (e.value().to_owned(), e.from(), e.target()),
        other => panic!("expected Error::LenientWidthOutOfRange, got {other:?}"),
    }
}

// --- the motivating cases ---------------------------------------------------

/// A `Byte` tag decoding into a declared `f32` field: the exact `ItemRotation`
/// case from the issue.
#[test]
fn a_byte_widens_into_a_declared_f32() {
    let doc = comp(&[("rotation", Value::Byte(2))]);
    assert_eq!(
        decode::<ItemRotation>(&doc).unwrap(),
        ItemRotation { rotation: 2.0 }
    );
}

/// A `Byte` tag decoding into a declared `i32` field.
#[test]
fn a_byte_widens_into_a_declared_i32() {
    let doc = comp(&[("count", Value::Byte(-3))]);
    assert_eq!(decode::<Count>(&doc).unwrap(), Count { count: -3 });
}

/// The field's own tag never stops working: listing extra tags is additive.
#[test]
fn the_declared_tag_still_decodes() {
    let doc = comp(&[("rotation", Value::Float(0.25))]);
    assert_eq!(
        decode::<ItemRotation>(&doc).unwrap(),
        ItemRotation { rotation: 0.25 }
    );
}

/// A tag that is neither the field's own nor in the list is the *old* error,
/// untouched: `lenient_width` narrows nothing.
#[test]
fn an_unlisted_tag_is_still_an_unexpected_type() {
    let doc = comp(&[("count", Value::Short(3))]);
    let err = decode::<Count>(&doc).unwrap_err();
    let Error::UnexpectedType(e) = &err else {
        panic!("expected Error::UnexpectedType, got {err:?}");
    };
    assert_eq!(e.expected(), nbtx::FieldType::Int);
    assert_eq!(e.found(), nbtx::FieldType::Short);
}

// --- what may and may not be converted --------------------------------------

/// Widening an integer can never fail; every value of the narrower type fits.
#[test]
fn every_integer_widens() {
    for v in [i8::MIN, -1, 0, 1, i8::MAX] {
        let doc = comp(&[("count", Value::Byte(v))]);
        assert_eq!(
            decode::<Count>(&doc).unwrap(),
            Count {
                count: i32::from(v)
            }
        );
    }
}

/// Narrowing an integer succeeds only inside the target's range.
#[test]
fn narrowing_an_integer_is_range_checked() {
    let doc = comp(&[("level", Value::Int(100))]);
    assert_eq!(decode::<Narrow>(&doc).unwrap(), Narrow { level: 100 });

    let doc = comp(&[("level", Value::Int(300))]);
    let err = decode::<Narrow>(&doc).unwrap_err();
    assert_eq!(
        out_of_range(&err),
        ("300".to_owned(), nbtx::FieldType::Int, "i8")
    );
}

/// An integer reaches a float only if the float reproduces it exactly — the
/// test is on the significand, not on the magnitude, so 2^24 passes and
/// 2^24 + 1 does not while a much larger power of two passes again.
#[test]
fn an_integer_reaches_a_float_only_when_exactly_representable() {
    let doc = comp(&[("value", Value::Long(1 << 24))]);
    assert_eq!(
        decode::<Wide>(&doc).unwrap(),
        Wide {
            value: 16_777_216.0
        }
    );

    let doc = comp(&[("value", Value::Long((1 << 24) + 1))]);
    let err = decode::<Wide>(&doc).unwrap_err();
    assert_eq!(out_of_range(&err).2, "f32");

    // 2^62 needs a single mantissa bit, so it converts exactly even though it
    // is vastly larger than the value that failed above.
    let doc = comp(&[("value", Value::Long(1 << 62))]);
    assert_eq!(
        decode::<Wide>(&doc).unwrap(),
        Wide {
            value: 4_611_686_018_427_387_904.0
        }
    );
}

/// A float reaches an integer only if it is whole and in range.
#[test]
fn a_float_reaches_an_integer_only_when_whole() {
    let doc = comp(&[("ticks", Value::Float(3.0))]);
    assert_eq!(decode::<FromFloat>(&doc).unwrap(), FromFloat { ticks: 3 });

    let doc = comp(&[("ticks", Value::Float(0.5))]);
    let err = decode::<FromFloat>(&doc).unwrap_err();
    assert_eq!(out_of_range(&err).2, "i32");

    // Whole, but far outside `i32`.
    let doc = comp(&[("ticks", Value::Double(1e18))]);
    let err = decode::<FromFloat>(&doc).unwrap_err();
    assert_eq!(out_of_range(&err).2, "i32");

    // Not finite: neither whole nor in range.
    let doc = comp(&[("ticks", Value::Double(f64::INFINITY))]);
    let err = decode::<FromFloat>(&doc).unwrap_err();
    assert_eq!(out_of_range(&err).2, "i32");
}

/// Narrowing a float succeeds only when the value survives the precision.
#[test]
fn narrowing_a_float_is_precision_checked() {
    let doc = comp(&[("value", Value::Double(0.5))]);
    assert_eq!(decode::<Wide>(&doc).unwrap(), Wide { value: 0.5 });

    let doc = comp(&[("value", Value::Double(0.1))]);
    let err = decode::<Wide>(&doc).unwrap_err();
    assert_eq!(out_of_range(&err).2, "f32");
}

// --- sequences and options --------------------------------------------------

/// One field-level declaration widens every element of a `Vec`, every element
/// of an array, and the inside of an `Option`, uniformly.
#[test]
fn one_declaration_widens_every_element() {
    let doc = comp(&[
        ("ids", Value::List(vec![Value::Byte(1), Value::Byte(-2)])),
        ("fixed", Value::ByteArray(vec![3, 4])),
        ("maybe", Value::Byte(5)),
    ]);
    assert_eq!(
        decode::<Seqs>(&doc).unwrap(),
        Seqs {
            ids: vec![1, -2],
            fixed: [3, 4],
            maybe: Some(5),
        }
    );
}

/// An absent `Option` is still simply `None`; the attribute changes nothing
/// about presence.
#[test]
fn a_missing_option_is_still_none() {
    let doc = comp(&[
        ("ids", Value::IntArray(vec![7])),
        ("fixed", Value::IntArray(vec![8, 9])),
    ]);
    assert_eq!(
        decode::<Seqs>(&doc).unwrap(),
        Seqs {
            ids: vec![7],
            fixed: [8, 9],
            maybe: None,
        }
    );
}

/// One bad element fails the whole decode rather than being dropped.
#[test]
fn an_unrepresentable_element_fails_the_sequence() {
    let doc = comp(&[("levels", Value::IntArray(vec![1, 300]))]);
    let err = decode::<Narrows>(&doc).unwrap_err();
    assert_eq!(out_of_range(&err).0, "300");
}

// --- misplacement -----------------------------------------------------------

#[track_caller]
fn invalid_placement<'f, T: Facet<'f> + PartialEq + std::fmt::Debug>(doc: &Value, field: &str) {
    let err = decode::<T>(doc).unwrap_err();
    let Error::InvalidLenientWidth(e) = &err else {
        panic!("expected Error::InvalidLenientWidth, got {err:?}");
    };
    assert_eq!(e.field(), field);
}

/// A nested compound has nothing to widen, so the declaration is reported the
/// first time the field is decoded — even though the document itself is
/// perfectly well formed.
#[test]
fn a_struct_field_cannot_be_widened() {
    let doc = comp(&[("inner", comp(&[("a", Value::Int(1))]))]);
    invalid_placement::<BadCompound>(&doc, "inner");
}

#[test]
fn a_vec_of_structs_cannot_be_widened() {
    let doc = comp(&[("inners", Value::List(vec![comp(&[("a", Value::Int(1))])]))]);
    invalid_placement::<BadVecOfStruct>(&doc, "inners");
}

/// `bool` is untouched by this attribute: its `Byte == 1` truthiness rule is a
/// separate concern, and naming it here is a mistake worth reporting.
#[test]
fn a_bool_cannot_be_widened() {
    let doc = comp(&[("flag", Value::Byte(1))]);
    invalid_placement::<BadBool>(&doc, "flag");
}

#[test]
fn a_string_cannot_be_widened() {
    let doc = comp(&[("name", Value::String("x".into()))]);
    invalid_placement::<BadString>(&doc, "name");
}

// --- enums ------------------------------------------------------------------

/// `variant_as` says what the discriminant normally is; `lenient_width` says
/// what else is accepted for it.
#[test]
fn an_enum_discriminant_widens() {
    let doc = comp(&[("difficulty", Value::Byte(3))]);
    assert_eq!(
        decode::<Level>(&doc).unwrap(),
        Level {
            difficulty: Difficulty::Hard
        }
    );

    // The declared mode still works, of course.
    let doc = comp(&[("difficulty", Value::Int(0))]);
    assert_eq!(
        decode::<Level>(&doc).unwrap(),
        Level {
            difficulty: Difficulty::Peaceful
        }
    );
}

/// An unlisted tag on an enum is still the plain tag mismatch.
#[test]
fn an_unlisted_tag_on_an_enum_is_an_unexpected_type() {
    let doc = comp(&[("difficulty", Value::Short(3))]);
    let err = decode::<Level>(&doc).unwrap_err();
    assert!(matches!(err, Error::UnexpectedType(_)), "got {err:?}");
}

/// A discriminant that arrives in an accepted tag but does not fit the declared
/// mode is refused rather than truncated into a different variant.
#[test]
fn a_widened_discriminant_is_range_checked() {
    assert_eq!(from_value::<Slot>(Value::Int(2)).unwrap(), Slot::Second);

    let err = from_value::<Slot>(Value::Int(300)).unwrap_err();
    assert_eq!(out_of_range(&err).2, "i8");
}

/// A number no variant claims is still rejected after widening, exactly as it
/// is at the declared width.
#[test]
fn a_widened_discriminant_must_still_name_a_variant() {
    let err = from_value::<Slot>(Value::Int(9)).unwrap_err();
    assert!(
        !matches!(err, Error::LenientWidthOutOfRange(_)),
        "got {err:?}"
    );
}

/// `str` mode carries names, not numbers, so widening it is meaningless.
#[test]
fn lenient_width_is_refused_on_a_str_mode_enum() {
    let err = from_value::<BadStrEnum>(Value::String("Only".into())).unwrap_err();
    let Error::InvalidLenientWidth(e) = &err else {
        panic!("expected Error::InvalidLenientWidth, got {err:?}");
    };
    assert_eq!(e.container(), "BadStrEnum");
}

// --- decode only ------------------------------------------------------------

/// Encoding never consults the list. A value decoded from a `Byte` is written
/// back as the field's own `Float`, so the attribute normalises a document
/// rather than round-tripping its original shape.
#[test]
fn re_encoding_writes_the_fields_own_tag() {
    let doc = comp(&[("rotation", Value::Byte(2))]);
    let decoded = decode::<ItemRotation>(&doc).unwrap();

    let re_encoded = to_value(&decoded).unwrap();
    assert_eq!(re_encoded, comp(&[("rotation", Value::Float(2.0))]));
    assert_ne!(re_encoded, doc, "the narrower tag must not survive");
}

/// The same, byte for byte: the re-encoded document is identical to one built
/// from a `Float` in the first place.
#[cfg(feature = "nbt")]
#[test]
fn re_encoded_bytes_match_the_natural_encoding() {
    let lenient = comp(&[("rotation", Value::Byte(2))]);
    let bytes = nbtx::to_be_bytes(&lenient).unwrap();
    let decoded: ItemRotation = nbtx::from_be_bytes(&mut bytes.as_slice()).unwrap();

    let natural = comp(&[("rotation", Value::Float(2.0))]);
    assert_eq!(
        nbtx::to_be_bytes(&decoded).unwrap(),
        nbtx::to_be_bytes(&natural).unwrap()
    );
}

// --- SNBT -------------------------------------------------------------------

/// SNBT is already width-tolerant — `parse_int`/`parse_float` drop a literal's
/// suffix and read at the *field's* width — so the attribute only has to widen
/// what that rejects. An integer literal too large for the declared type is one
/// such case, and it now reports the widening failure rather than a bare parse
/// error.
#[cfg(feature = "snbt")]
#[test]
fn snbt_reports_an_out_of_range_literal_as_a_widening_failure() {
    let err = nbtx::from_string::<Narrow>("{level: 300}").unwrap_err();
    assert_eq!(out_of_range(&err).2, "i8");

    // Without a declaration the old parse error stands, unchanged.
    #[derive(Facet, Debug, PartialEq)]
    struct Strict {
        level: i8,
    }
    let err = nbtx::from_string::<Strict>("{level: 300}").unwrap_err();
    assert!(matches!(err, Error::ParseIntError(_)), "got {err:?}");
}

/// A decimal literal read into an integer field: the literal's own suffix gives
/// its tag (`3.0` is a `Double`, `3.0f` a `Float`), and the declaration decides
/// whether that tag is accepted.
#[cfg(feature = "snbt")]
#[test]
fn snbt_widens_a_float_literal_into_an_integer_field() {
    assert_eq!(
        nbtx::from_string::<FromFloat>("{ticks: 3.0}").unwrap(),
        FromFloat { ticks: 3 }
    );
    assert_eq!(
        nbtx::from_string::<FromFloat>("{ticks: 3.0f}").unwrap(),
        FromFloat { ticks: 3 }
    );

    let err = nbtx::from_string::<FromFloat>("{ticks: 3.5}").unwrap_err();
    assert_eq!(out_of_range(&err).2, "i32");
}

/// The textual codec applies the same placement rule as the other two.
#[cfg(feature = "snbt")]
#[test]
fn snbt_refuses_a_misplaced_declaration() {
    let err = nbtx::from_string::<BadCompound>("{inner: {a: 1}}").unwrap_err();
    assert!(matches!(err, Error::InvalidLenientWidth(_)), "got {err:?}");
}

/// And the same enum rule. A literal that fits the declared mode is read at
/// that width as before; one that does not is widened in and then range-checked,
/// so it reports the widening failure rather than a bare parse error.
#[cfg(feature = "snbt")]
#[test]
fn snbt_range_checks_a_widened_enum_discriminant() {
    assert_eq!(nbtx::from_string::<Slot>("2").unwrap(), Slot::Second);

    let err = nbtx::from_string::<Slot>("300").unwrap_err();
    assert_eq!(out_of_range(&err).2, "i8");
}

// --- exact representability, exhaustively ------------------------------------

/// `i32 -> f32`: the real risk zone. Most `i32` values need more than 24
/// significant bits and are *not* exactly representable in `f32`; the
/// boundary is 2^24, tested on both sides and both signs.
#[test]
fn i32_reaches_f32_only_at_the_24_bit_boundary() {
    let doc = comp(&[("value", Value::Int(1 << 24))]);
    assert_eq!(
        decode::<FromI32>(&doc).unwrap(),
        FromI32 {
            value: 16_777_216.0
        }
    );

    let doc = comp(&[("value", Value::Int((1 << 24) + 1))]);
    let err = decode::<FromI32>(&doc).unwrap_err();
    assert_eq!(out_of_range(&err).2, "f32");

    // The negative boundary is symmetric.
    let doc = comp(&[("value", Value::Int(-(1 << 24)))]);
    assert_eq!(
        decode::<FromI32>(&doc).unwrap(),
        FromI32 {
            value: -16_777_216.0
        }
    );

    let doc = comp(&[("value", Value::Int(-((1 << 24) + 1)))]);
    let err = decode::<FromI32>(&doc).unwrap_err();
    assert_eq!(out_of_range(&err).2, "f32");
}

/// `i64 -> f32`: the negative side of the boundary
/// `an_integer_reaches_a_float_only_when_exactly_representable` already pins
/// on the positive side.
#[test]
fn i64_reaches_f32_at_the_negative_24_bit_boundary() {
    let doc = comp(&[("value", Value::Long(-(1 << 24)))]);
    assert_eq!(
        decode::<Wide>(&doc).unwrap(),
        Wide {
            value: -16_777_216.0
        }
    );

    let doc = comp(&[("value", Value::Long(-((1 << 24) + 1)))]);
    let err = decode::<Wide>(&doc).unwrap_err();
    assert_eq!(out_of_range(&err).2, "f32");
}

/// `i32 -> f64`: always exact. `f64`'s 53-bit significand comfortably covers
/// every `i32`, including values nowhere near exact in `f32`.
#[test]
fn i32_always_reaches_f64_exactly() {
    for v in [i32::MAX, i32::MIN, (1 << 24) + 1, -((1 << 24) + 1)] {
        let doc = comp(&[("value", Value::Int(v))]);
        assert_eq!(
            decode::<WideF64>(&doc).unwrap(),
            WideF64 {
                value: f64::from(v)
            }
        );
    }
}

/// `i64 -> f64`: the real risk zone at this width, mirroring `i32 -> f32`. The
/// boundary is 2^53, not 2^24.
#[test]
fn i64_reaches_f64_only_at_the_53_bit_boundary() {
    let doc = comp(&[("value", Value::Long(1i64 << 53))]);
    assert_eq!(
        decode::<WideF64>(&doc).unwrap(),
        WideF64 {
            value: 9_007_199_254_740_992.0
        }
    );

    let doc = comp(&[("value", Value::Long((1i64 << 53) + 1))]);
    let err = decode::<WideF64>(&doc).unwrap_err();
    assert_eq!(out_of_range(&err).2, "f64");

    let doc = comp(&[("value", Value::Long(-(1i64 << 53)))]);
    assert_eq!(
        decode::<WideF64>(&doc).unwrap(),
        WideF64 {
            value: -9_007_199_254_740_992.0
        }
    );

    let doc = comp(&[("value", Value::Long(-((1i64 << 53) + 1)))]);
    let err = decode::<WideF64>(&doc).unwrap_err();
    assert_eq!(out_of_range(&err).2, "f64");

    // A far larger magnitude with few significant bits still converts
    // exactly, the same non-monotonic-in-magnitude shape
    // `an_integer_reaches_a_float_only_when_exactly_representable` pins for
    // `f32`.
    let doc = comp(&[("value", Value::Long(1i64 << 62))]);
    assert_eq!(
        decode::<WideF64>(&doc).unwrap(),
        WideF64 {
            value: 4_611_686_018_427_387_904.0
        }
    );
}

/// `f64 -> f32`: a value too large in *magnitude* for `f32` to hold at all is
/// refused, not silently saturated to infinity the way a bare `as` cast would.
#[test]
fn f64_overflowing_f32_magnitude_is_refused() {
    let doc = comp(&[("value", Value::Double(1e300))]);
    let err = decode::<Wide>(&doc).unwrap_err();
    assert_eq!(out_of_range(&err).2, "f32");
}

/// `NaN` is never equal to itself, so the `f64 -> f32` exactness check
/// (`f64::from(narrowed) == v`) has to admit it explicitly rather than
/// spuriously rejecting every `NaN` as "not exact".
#[test]
fn f64_nan_survives_narrowing_to_f32() {
    // Not routed through the shared `decode` helper: it cross-checks with
    // `assert_eq!`, and `NaN != NaN` would make an entirely correct pair of
    // `NaN` outputs look like a disagreement.
    let doc = comp(&[("value", Value::Double(f64::NAN))]);
    let out = from_value::<Wide>(doc.clone()).unwrap();
    assert!(out.value.is_nan());

    #[cfg(feature = "nbt")]
    {
        let bytes = nbtx::to_be_bytes(&doc).unwrap();
        let out: Wide = nbtx::from_be_bytes(&mut bytes.as_slice()).unwrap();
        assert!(out.value.is_nan());
    }
}

/// A float reaching an integer is refused — never panicking, never silently
/// producing a bogus number — for every non-finite input: `is_finite()` short
/// circuits before `.fract()` is even called, so `NaN`'s and infinity's
/// surprising `.fract()` behaviour is never actually exercised.
#[test]
fn non_finite_floats_never_reach_an_integer() {
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let doc = comp(&[("ticks", Value::Double(v))]);
        let err = decode::<FromFloat>(&doc).unwrap_err();
        assert_eq!(out_of_range(&err).2, "i32");
    }
}

// --- enum discriminant sign / bit pattern ------------------------------------

/// A widened discriminant for an *unsigned* mode is read through the mode's
/// own signed tag width, not through the literal decimal value: mode `u8`'s
/// tag is `Byte` (`i8`), so 200 (which does not fit `i8`) is refused even
/// though it fits comfortably in the wider `Short`/`Int` tag it arrived in,
/// while -56 — the bit pattern of unsigned 200 in a byte — does fit `i8` and
/// resolves to discriminant 200. This is the same convention the *natural*
/// (non-lenient) `Byte(-56)` case already uses (see `VariantAs::narrow`), so
/// widening does not add a second way to reach the same discriminant.
#[test]
fn a_widened_unsigned_discriminant_follows_the_bit_pattern_not_the_literal() {
    let err = from_value::<U8Mode>(Value::Short(200)).unwrap_err();
    assert_eq!(out_of_range(&err).2, "i8");
    let err = from_value::<U8Mode>(Value::Int(200)).unwrap_err();
    assert_eq!(out_of_range(&err).2, "i8");

    assert_eq!(
        from_value::<U8Mode>(Value::Short(-56)).unwrap(),
        U8Mode::TwoHundred
    );
    assert_eq!(
        from_value::<U8Mode>(Value::Int(-56)).unwrap(),
        U8Mode::TwoHundred
    );
}

// --- more misplacement --------------------------------------------------

#[test]
fn a_value_field_cannot_be_widened() {
    let doc = comp(&[("v", Value::Byte(1))]);
    invalid_placement::<BadValue>(&doc, "v");
}

#[test]
fn an_option_of_struct_cannot_be_widened() {
    let doc = comp(&[("inner", comp(&[("a", Value::Int(1))]))]);
    invalid_placement::<BadOptionStruct>(&doc, "inner");
}

#[test]
fn a_bare_u8_cannot_be_widened() {
    let doc = comp(&[("v", Value::Byte(1))]);
    invalid_placement::<BadU8>(&doc, "v");
}

#[test]
fn a_byte_array_field_cannot_be_widened() {
    let doc = comp(&[("data", Value::ByteArray(vec![1, 2, 3]))]);
    invalid_placement::<BadByteArray>(&doc, "data");
}

// --- multiple fields, nested containers, redundant declarations -------------

/// Each field's declaration is independent: one field's extra tags must not
/// leak into another field's.
#[test]
fn multiple_lenient_fields_do_not_interfere() {
    let doc = comp(&[("a", Value::Byte(5)), ("b", Value::Double(2.5))]);
    assert_eq!(
        decode::<MultiLenient>(&doc).unwrap(),
        MultiLenient { a: 5, b: 2.5 }
    );

    // `a` only tolerates `Byte`; giving it a `Double` (which only `b`
    // tolerates) is still the plain tag mismatch, not silently accepted.
    let doc = comp(&[("a", Value::Double(5.0)), ("b", Value::Double(2.5))]);
    let err = decode::<MultiLenient>(&doc).unwrap_err();
    assert!(matches!(err, Error::UnexpectedType(_)), "got {err:?}");
}

/// One declaration widens a scalar reached through *two* nested containers at
/// once (`Vec<Option<f32>>`), not just one.
#[test]
fn one_declaration_widens_through_a_vec_of_options() {
    let doc = comp(&[("values", Value::List(vec![Value::Byte(1), Value::Byte(-2)]))]);
    assert_eq!(
        decode::<NestedOpt>(&doc).unwrap(),
        NestedOpt {
            values: vec![Some(1.0), Some(-2.0)]
        }
    );
}

/// Naming a field's own type in its `lenient_width` list is redundant but
/// harmless: the natural-tag path is tried first regardless.
#[test]
fn listing_the_fields_own_type_is_harmless() {
    #[derive(Facet, Debug, PartialEq)]
    struct Redundant {
        #[facet(nbtx::lenient_width(f32, i8))]
        value: f32,
    }
    let doc = comp(&[("value", Value::Float(1.5))]);
    assert_eq!(decode::<Redundant>(&doc).unwrap(), Redundant { value: 1.5 });
}

// --- decode only, enum edition ------------------------------------------

/// Re-encoding a leniently decoded enum discriminant uses the enum's declared
/// mode, never the tag it happened to arrive in, exactly as a scalar field
/// does.
#[cfg(feature = "nbt")]
#[test]
fn re_encoded_enum_discriminant_uses_its_declared_mode() {
    let lenient = comp(&[("difficulty", Value::Byte(3))]);
    let bytes = nbtx::to_be_bytes(&lenient).unwrap();
    let decoded: Level = nbtx::from_be_bytes(&mut bytes.as_slice()).unwrap();

    let natural = comp(&[("difficulty", Value::Int(3))]);
    assert_eq!(
        nbtx::to_be_bytes(&decoded).unwrap(),
        nbtx::to_be_bytes(&natural).unwrap()
    );
}

// --- SNBT: the "necessarily thinner" asymmetry -------------------------

/// `lenient_width(f32)` alone cannot accept a bare (suffixless) decimal
/// literal: SNBT's own grammar already types it as `Double` before the
/// attribute is consulted, so only the explicitly `f32`-suffixed spelling
/// (`3.0f`) reaches this field.
#[cfg(feature = "snbt")]
#[test]
fn snbt_lenient_width_f32_alone_does_not_accept_a_bare_decimal_literal() {
    let err = nbtx::from_string::<SnbtOnlyF32>("{ticks: 3.0}").unwrap_err();
    assert!(matches!(err, Error::ParseIntError(_)), "got {err:?}");

    assert_eq!(
        nbtx::from_string::<SnbtOnlyF32>("{ticks: 3.0f}").unwrap(),
        SnbtOnlyF32 { ticks: 3 }
    );
}

/// The mirror image: `lenient_width(f64)` alone accepts a bare `3.0` but not
/// an explicitly `f32`-suffixed `3.0f` — the two declarations are not
/// interchangeable, and a field must name *both* to accept either spelling.
#[cfg(feature = "snbt")]
#[test]
fn snbt_lenient_width_f64_alone_does_not_accept_an_f32_suffixed_literal() {
    assert_eq!(
        nbtx::from_string::<SnbtOnlyF64>("{ticks: 3.0}").unwrap(),
        SnbtOnlyF64 { ticks: 3 }
    );

    let err = nbtx::from_string::<SnbtOnlyF64>("{ticks: 3.0f}").unwrap_err();
    assert!(matches!(err, Error::ParseIntError(_)), "got {err:?}");
}
