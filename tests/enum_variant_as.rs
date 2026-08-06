//! `#[facet(nbtx::variant_as(<mode>))]`: how an enum's variants reach the wire.
//!
//! The attribute is **mandatory** on every enum nbtx touches — there is no
//! default — and picks one of ten forms: the variant's name as a `String` tag
//! (`str`), or its discriminant as a fixed-width scalar (`u8`, `i8`, `u16`,
//! `i16`, `u32`, `i32`, `u64`, `i64`).
//!
//! Three properties carry this file:
//!
//! * **Agreement.** All three codecs — binary (in each of its three
//!   endiannesses), textual, and the direct [`nbtx::to_value`] conversion —
//!   apply the same mode to the same enum, so a value written by one is read by
//!   any other.
//! * **Per-variant control comes from Rust and from facet, not from nbtx.**
//!   `str` mode honours `#[facet(rename = "...")]`; the integer modes write the
//!   variant's own discriminant, including a custom `= <int>` assignment.
//! * **Nothing is silently narrowed.** A discriminant that does not fit the
//!   declared width, and an enum with no declared width at all, are errors.
//!
//! The conversion tests need no features; the byte- and text-level ones are
//! gated on `nbt`/`snbt` respectively.

use facet::Facet;
use nbtx::{Error, Value, from_value, to_value};

// --- the enums under test ---------------------------------------------------
//
// One enum per mode, each with a low variant and a "hard" one whose
// discriminant exercises the edge of that mode's width. The `#[repr(...)]` is
// whatever Rust needs to hold the discriminant; it is deliberately *not* what
// decides the wire width (see `wire_width_is_independent_of_the_rust_repr`).

#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(nbtx::variant_as(str))]
#[repr(u8)]
enum ModeStr {
    Survival,
    /// Renamed: the wire carries `creative`, not `Creative`.
    #[facet(rename = "creative")]
    Creative,
}

#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(nbtx::variant_as(u8))]
#[repr(u8)]
enum ModeU8 {
    Low = 1,
    /// 200 is negative when read as the signed `Byte` tag that carries it.
    High = 200,
}

#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(nbtx::variant_as(i8))]
#[repr(i8)]
enum ModeI8 {
    Low = 1,
    High = -100,
}

#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(nbtx::variant_as(u16))]
#[repr(u16)]
enum ModeU16 {
    Low = 1,
    High = 40_000,
}

#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(nbtx::variant_as(i16))]
#[repr(i16)]
enum ModeI16 {
    Low = 1,
    High = -30_000,
}

#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(nbtx::variant_as(u32))]
#[repr(u32)]
enum ModeU32 {
    Low = 1,
    High = 3_000_000_000,
}

#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(nbtx::variant_as(i32))]
#[repr(i32)]
enum ModeI32 {
    Low = 1,
    High = -2_000_000_000,
}

#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(nbtx::variant_as(u64))]
#[repr(u64)]
enum ModeU64 {
    Low = 1,
    High = 9_000_000_000_000,
}

#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(nbtx::variant_as(i64))]
#[repr(i64)]
enum ModeI64 {
    Low = 1,
    High = -9_000_000_000_000,
}

/// A struct field, not just a bare root: the enum has to work where it is
/// actually used, keyed inside a compound.
///
/// Built only by the binary and textual codec tests below (`#[cfg]`-gated on
/// `nbt`/`snbt`); under `--no-default-features` neither exists to call it.
#[cfg(any(feature = "nbt", feature = "snbt"))]
#[derive(Facet, Debug, Clone, PartialEq)]
struct Settings {
    mode: ModeStr,
    difficulty: ModeU8,
    seed: ModeI64,
}

#[cfg(any(feature = "nbt", feature = "snbt"))]
fn settings() -> Settings {
    Settings {
        mode: ModeStr::Creative,
        difficulty: ModeU8::High,
        seed: ModeI64::High,
    }
}

/// Runs `$check!(Type, value)` once per mode, on the "hard" variant of each.
macro_rules! for_each_mode {
    ($check:ident) => {
        $check!(ModeStr, ModeStr::Creative);
        $check!(ModeU8, ModeU8::High);
        $check!(ModeI8, ModeI8::High);
        $check!(ModeU16, ModeU16::High);
        $check!(ModeI16, ModeI16::High);
        $check!(ModeU32, ModeU32::High);
        $check!(ModeI32, ModeI32::High);
        $check!(ModeU64, ModeU64::High);
        $check!(ModeI64, ModeI64::High);
        // ...and once on a low variant, so a mode that happened to work only
        // for its edge case is still caught.
        $check!(ModeStr, ModeStr::Survival);
        $check!(ModeU8, ModeU8::Low);
        $check!(ModeI8, ModeI8::Low);
        $check!(ModeU16, ModeU16::Low);
        $check!(ModeI16, ModeI16::Low);
        $check!(ModeU32, ModeU32::Low);
        $check!(ModeI32, ModeI32::Low);
        $check!(ModeU64, ModeU64::Low);
        $check!(ModeI64, ModeI64::Low);
    };
}

// --- to_value / from_value --------------------------------------------------

/// Every mode survives the `Value` conversion, in both directions.
#[test]
fn every_mode_roundtrips_through_value() {
    macro_rules! check {
        ($ty:ty, $value:expr) => {{
            let original: $ty = $value;
            let value = to_value(&original).expect("a declared enum converts");
            let back: $ty = from_value(value).expect("and converts back");
            assert_eq!(back, original, "{} lost its variant", stringify!($ty));
        }};
    }
    for_each_mode!(check);
}

/// The tag each mode produces, stated once and asserted directly: the mode
/// decides the tag, and a `u8`/`i8` enum is a `Byte` exactly as a `u8` field is.
#[test]
fn each_mode_produces_the_tag_its_width_implies() {
    assert_eq!(
        to_value(&ModeStr::Survival).unwrap(),
        Value::String("Survival".into())
    );
    assert_eq!(to_value(&ModeU8::Low).unwrap(), Value::Byte(1));
    assert_eq!(to_value(&ModeI8::Low).unwrap(), Value::Byte(1));
    assert_eq!(to_value(&ModeU16::Low).unwrap(), Value::Short(1));
    assert_eq!(to_value(&ModeI16::Low).unwrap(), Value::Short(1));
    assert_eq!(to_value(&ModeU32::Low).unwrap(), Value::Int(1));
    assert_eq!(to_value(&ModeI32::Low).unwrap(), Value::Int(1));
    assert_eq!(to_value(&ModeU64::Low).unwrap(), Value::Long(1));
    assert_eq!(to_value(&ModeI64::Low).unwrap(), Value::Long(1));
}

/// The unsigned modes keep the *bit pattern*, not the number: the NBT scalar
/// tags are all signed, so discriminant 200 in `u8` mode is `Byte(-56)` — the
/// same convention a plain `u8` field already uses — and reading it back
/// zero-extends it into 200 again rather than selecting nothing.
#[test]
fn an_unsigned_discriminant_keeps_its_bit_pattern() {
    assert_eq!(to_value(&ModeU8::High).unwrap(), Value::Byte(-56));
    assert_eq!(
        from_value::<ModeU8>(Value::Byte(-56)).unwrap(),
        ModeU8::High
    );

    assert_eq!(to_value(&ModeU16::High).unwrap(), Value::Short(-25_536));
    assert_eq!(
        from_value::<ModeU16>(Value::Short(-25_536)).unwrap(),
        ModeU16::High
    );

    assert_eq!(
        to_value(&ModeU32::High).unwrap(),
        Value::Int(-1_294_967_296)
    );
    assert_eq!(
        from_value::<ModeU32>(Value::Int(-1_294_967_296)).unwrap(),
        ModeU32::High
    );

    // A signed mode is the plain reading, with no reinterpretation.
    assert_eq!(to_value(&ModeI8::High).unwrap(), Value::Byte(-100));
    assert_eq!(to_value(&ModeI16::High).unwrap(), Value::Short(-30_000));
    assert_eq!(
        to_value(&ModeI32::High).unwrap(),
        Value::Int(-2_000_000_000)
    );
    assert_eq!(
        to_value(&ModeI64::High).unwrap(),
        Value::Long(-9_000_000_000_000)
    );
}

/// A custom `= <int>` discriminant is what a document's numbers are pinned to;
/// nbtx has no attribute of its own for that, and needs none.
#[test]
fn a_custom_rust_discriminant_is_the_number_on_the_wire() {
    assert_eq!(to_value(&ModeU8::Low).unwrap(), Value::Byte(1));
    assert_eq!(
        to_value(&ModeU64::High).unwrap(),
        Value::Long(9_000_000_000_000)
    );
    // Reordering or renaming variants cannot move these numbers, but changing
    // the assignment does — which is exactly why they are written down.
    assert_eq!(
        from_value::<ModeU64>(Value::Long(9_000_000_000_000)).unwrap(),
        ModeU64::High
    );
    assert!(
        from_value::<ModeU64>(Value::Long(2)).is_err(),
        "no variant has discriminant 2"
    );
}

/// `str` mode uses the variant's *effective* name, so
/// `#[facet(rename = "...")]` decides both what is written and what is
/// accepted — the same rule struct fields already follow.
#[test]
fn str_mode_honours_a_variant_rename() {
    assert_eq!(
        to_value(&ModeStr::Creative).unwrap(),
        Value::String("creative".into()),
        "the rename, not the Rust identifier, goes on the wire"
    );
    assert_eq!(
        from_value::<ModeStr>(Value::String("creative".into())).unwrap(),
        ModeStr::Creative
    );
    assert!(
        from_value::<ModeStr>(Value::String("Creative".into())).is_err(),
        "the renamed-away Rust identifier is not accepted"
    );
    // An un-renamed variant is unaffected.
    assert_eq!(
        to_value(&ModeStr::Survival).unwrap(),
        Value::String("Survival".into())
    );
}

/// The wire width is the attribute's business alone: this enum is a `u8` in
/// memory and a `Long` on the wire.
#[test]
fn wire_width_is_independent_of_the_rust_repr() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(nbtx::variant_as(i64))]
    #[repr(u8)]
    enum Narrow {
        Only = 7,
    }
    assert_eq!(to_value(&Narrow::Only).unwrap(), Value::Long(7));
    assert_eq!(from_value::<Narrow>(Value::Long(7)).unwrap(), Narrow::Only);
}

/// `usize`/`isize` are refused however plausible they look: their width varies
/// by target, and a document that decodes differently on a 32-bit machine is not
/// a wire format. The attribute takes any type syntactically, so this is a
/// runtime error rather than a compile one.
#[test]
fn a_target_dependent_width_is_refused() {
    #[derive(Facet, Debug)]
    #[facet(nbtx::variant_as(usize))]
    #[repr(u8)]
    enum PlatformSized {
        Only = 1,
    }
    match to_value(&PlatformSized::Only) {
        Err(Error::Unsupported(e)) => assert!(e.operation().contains("variant_as"), "{e:?}"),
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

/// A tag the mode did not ask for is a type error, not a coercion: an `i32`-mode
/// enum will not accept the `Byte` a `u8`-mode one would have written.
#[test]
fn a_mode_only_accepts_its_own_tag() {
    match from_value::<ModeI32>(Value::Byte(1)) {
        Err(Error::UnexpectedType(e)) => {
            assert_eq!(e.expected(), nbtx::FieldType::Int);
            assert_eq!(e.found(), nbtx::FieldType::Byte);
        }
        other => panic!("expected UnexpectedType, got {other:?}"),
    }
    match from_value::<ModeStr>(Value::Byte(1)) {
        Err(Error::UnexpectedType(e)) => assert_eq!(e.expected(), nbtx::FieldType::String),
        other => panic!("expected UnexpectedType, got {other:?}"),
    }
}

// --- errors -----------------------------------------------------------------

/// A discriminant that does not fit the declared width is refused. Truncating it
/// would write a number that decodes as a *different* variant.
#[test]
fn a_discriminant_that_does_not_fit_is_refused() {
    #[derive(Facet, Debug)]
    #[facet(nbtx::variant_as(u8))]
    #[repr(u16)]
    enum TooWide {
        Big = 500,
    }
    match to_value(&TooWide::Big) {
        Err(Error::DiscriminantOutOfRange(e)) => {
            assert_eq!(e.container(), "TooWide");
            assert_eq!(e.variant(), "Big");
            assert_eq!(e.discriminant(), 500);
            assert_eq!(e.mode(), "u8");
        }
        other => panic!("expected DiscriminantOutOfRange, got {other:?}"),
    }

    // A *negative* discriminant does not fit an unsigned mode either, however
    // few bits it needs.
    #[derive(Facet, Debug)]
    #[facet(nbtx::variant_as(u16))]
    #[repr(i16)]
    enum Negative {
        Below = -1,
    }
    assert!(matches!(
        to_value(&Negative::Below),
        Err(Error::DiscriminantOutOfRange(_))
    ));
}

/// The other variants of the same enum are unaffected: only the value actually
/// being written is range-checked.
#[test]
fn only_the_active_variant_is_range_checked() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(nbtx::variant_as(u8))]
    #[repr(u16)]
    enum Mixed {
        Fits = 3,
        DoesNot = 400,
    }
    assert_eq!(to_value(&Mixed::Fits).unwrap(), Value::Byte(3));
    assert!(matches!(
        to_value(&Mixed::DoesNot),
        Err(Error::DiscriminantOutOfRange(_))
    ));
}

/// An enum without the attribute is an error from the conversion, on both sides.
#[test]
fn an_enum_without_the_attribute_is_refused_by_the_conversion() {
    match to_value(&Undeclared::Survival) {
        Err(Error::MissingVariantAs(e)) => assert_eq!(e.container(), "Undeclared"),
        other => panic!("expected MissingVariantAs, got {other:?}"),
    }
    assert!(matches!(
        from_value::<Undeclared>(Value::String("Survival".into())),
        Err(Error::MissingVariantAs(_))
    ));
    // Also when it is merely a *field* of the value being converted.
    assert!(matches!(
        to_value(&WithUndeclared {
            mode: Undeclared::Survival
        }),
        Err(Error::MissingVariantAs(_))
    ));
}

#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum Undeclared {
    Survival,
}

#[derive(Facet, Debug, PartialEq)]
struct WithUndeclared {
    mode: Undeclared,
}

// --- the binary codec -------------------------------------------------------

#[cfg(feature = "nbt")]
mod binary {
    use super::*;
    use nbtx::{
        from_be_bytes, from_le_bytes, from_varint_bytes, to_be_bytes, to_le_bytes, to_varint_bytes,
    };

    macro_rules! for_each_endian {
        ($check:ident) => {
            $check!(to_be_bytes, from_be_bytes);
            $check!(to_le_bytes, from_le_bytes);
            $check!(to_varint_bytes, from_varint_bytes);
        };
    }

    /// Every mode round-trips through every endianness.
    #[test]
    fn every_mode_roundtrips_in_every_endianness() {
        macro_rules! per_endian {
            ($to:ident, $from:ident) => {{
                macro_rules! check {
                    ($ty:ty, $value:expr) => {{
                        let original: $ty = $value;
                        let bytes = $to(&original).expect("a declared enum encodes");
                        let back: $ty = $from(&mut bytes.as_slice()).expect("and decodes");
                        assert_eq!(
                            back,
                            original,
                            "{} lost its variant through {}",
                            stringify!($ty),
                            stringify!($to)
                        );
                    }};
                }
                for_each_mode!(check);
            }};
        }
        for_each_endian!(per_endian);
    }

    /// A struct field, which is where an enum actually lives: three modes at
    /// once, keyed inside a compound.
    #[test]
    fn an_enum_field_of_a_struct_roundtrips() {
        macro_rules! check {
            ($to:ident, $from:ident) => {{
                let original = settings();
                let bytes = $to(&original).unwrap();
                let back: Settings = $from(&mut bytes.as_slice()).unwrap();
                assert_eq!(back, original);
            }};
        }
        for_each_endian!(check);
    }

    /// The bytes are the same ones the equivalent scalar would have produced:
    /// the enum's tag byte and payload are indistinguishable from a `Byte`
    /// field holding the discriminant's bit pattern.
    #[test]
    fn the_bytes_match_the_equivalent_scalar() {
        #[derive(Facet, Debug)]
        struct AsEnum {
            v: ModeU8,
        }
        #[derive(Facet, Debug)]
        struct AsScalar {
            v: i8,
        }
        assert_eq!(
            to_be_bytes(&AsEnum { v: ModeU8::High }).unwrap(),
            to_be_bytes(&AsScalar { v: -56 }).unwrap(),
            "`variant_as(u8)` writes a plain Byte tag"
        );

        #[derive(Facet, Debug)]
        struct AsName {
            v: ModeStr,
        }
        #[derive(Facet, Debug)]
        struct AsString {
            v: String,
        }
        assert_eq!(
            to_be_bytes(&AsName {
                v: ModeStr::Creative
            })
            .unwrap(),
            to_be_bytes(&AsString {
                v: "creative".to_owned()
            })
            .unwrap(),
            "`variant_as(str)` writes a plain String tag holding the effective name"
        );
    }

    /// What the binary codec produces and what `to_value` produces agree, mode
    /// by mode — the property the whole `Value` conversion rests on.
    #[test]
    fn the_binary_codec_and_to_value_agree() {
        macro_rules! check {
            ($ty:ty, $value:expr) => {{
                let original: $ty = $value;
                let bytes = to_be_bytes(&original).unwrap();
                let decoded: Value = from_be_bytes(&mut bytes.as_slice()).unwrap();
                assert_eq!(
                    decoded,
                    to_value(&original).unwrap(),
                    "{} disagrees between the two paths",
                    stringify!($ty)
                );
            }};
        }
        for_each_mode!(check);
    }

    /// A missing attribute is an error on the binary path too — on write, and on
    /// read of a document that some other tool produced.
    #[test]
    fn an_enum_without_the_attribute_is_refused() {
        assert!(matches!(
            to_be_bytes(&Undeclared::Survival),
            Err(Error::MissingVariantAs(_))
        ));

        let doc = to_be_bytes(&"Survival".to_owned()).unwrap();
        assert!(matches!(
            from_be_bytes::<Undeclared>(&mut doc.as_slice()),
            Err(Error::MissingVariantAs(_))
        ));
    }

    /// An out-of-range discriminant is refused before any bytes are emitted.
    #[test]
    fn an_out_of_range_discriminant_is_refused() {
        #[derive(Facet, Debug)]
        #[facet(nbtx::variant_as(i8))]
        #[repr(i16)]
        enum TooWide {
            Big = 1000,
        }
        assert!(matches!(
            to_be_bytes(&TooWide::Big),
            Err(Error::DiscriminantOutOfRange(_))
        ));
    }

    /// A number that no variant claims is an error, not a default. Documents
    /// carry numbers now, so a stale one must not decode into an arbitrary
    /// variant.
    #[test]
    fn an_unknown_discriminant_is_rejected() {
        let doc = to_be_bytes(&7_i8).unwrap();
        assert!(from_be_bytes::<ModeU8>(&mut doc.as_slice()).is_err());
    }

    /// The tag has to match the declared mode: a `Byte` where the enum declared
    /// `i32` is a type error rather than a widening.
    #[test]
    fn a_tag_other_than_the_modes_own_is_rejected() {
        let doc = to_be_bytes(&1_i8).unwrap();
        match from_be_bytes::<ModeI32>(&mut doc.as_slice()) {
            Err(Error::UnexpectedType(e)) => {
                assert_eq!(e.expected(), nbtx::FieldType::Int);
                assert_eq!(e.found(), nbtx::FieldType::Byte);
            }
            other => panic!("expected UnexpectedType, got {other:?}"),
        }
    }

    /// A list of enums carries one element-type byte for the whole list, so the
    /// mode has to be answerable from the enum's shape alone — including for an
    /// empty list, where there is no element to ask.
    #[test]
    fn a_list_of_enums_roundtrips_including_when_empty() {
        #[derive(Facet, Debug, PartialEq)]
        struct Modes {
            all: Vec<ModeU8>,
        }
        let full = Modes {
            all: vec![ModeU8::Low, ModeU8::High],
        };
        let bytes = to_be_bytes(&full).unwrap();
        assert_eq!(from_be_bytes::<Modes>(&mut bytes.as_slice()).unwrap(), full);

        let empty = Modes { all: Vec::new() };
        let bytes = to_be_bytes(&empty).unwrap();
        let decoded: Value = from_be_bytes(&mut bytes.as_slice()).unwrap();
        let list = decoded.as_compound().unwrap()[bstr::BStr::new("all")].clone();
        assert_eq!(list, Value::List(Vec::new()));
        assert_eq!(
            from_be_bytes::<Modes>(&mut bytes.as_slice()).unwrap(),
            empty
        );
    }
}

// --- SNBT -------------------------------------------------------------------

#[cfg(feature = "snbt")]
mod text {
    use super::*;
    use nbtx::{from_string, to_string};

    /// Every mode round-trips through the textual codec.
    #[test]
    fn every_mode_roundtrips_through_snbt() {
        macro_rules! check {
            ($ty:ty, $value:expr) => {{
                let original: $ty = $value;
                let text = to_string(&original).expect("a declared enum renders");
                let back: $ty = from_string(&text).expect("and parses back");
                assert_eq!(
                    back,
                    original,
                    "{} lost its variant through `{}`",
                    stringify!($ty),
                    text
                );
            }};
        }
        for_each_mode!(check);
    }

    /// The literal each mode renders, spelled out: exactly the SNBT syntax the
    /// equivalent scalar uses, suffix and all, so the text stays readable by
    /// anything that reads SNBT.
    #[test]
    fn each_mode_renders_the_literal_of_its_tag() {
        assert_eq!(to_string(&ModeStr::Survival).unwrap(), "\"Survival\"");
        assert_eq!(to_string(&ModeStr::Creative).unwrap(), "\"creative\"");
        assert_eq!(to_string(&ModeU8::Low).unwrap(), "1b");
        assert_eq!(to_string(&ModeI8::Low).unwrap(), "1b");
        assert_eq!(to_string(&ModeU16::Low).unwrap(), "1s");
        assert_eq!(to_string(&ModeI16::Low).unwrap(), "1s");
        // An `Int` is the suffix-less literal in SNBT.
        assert_eq!(to_string(&ModeU32::Low).unwrap(), "1");
        assert_eq!(to_string(&ModeI32::Low).unwrap(), "1");
        assert_eq!(to_string(&ModeU64::Low).unwrap(), "1l");
        assert_eq!(to_string(&ModeI64::Low).unwrap(), "1l");

        // The unsigned modes render the signed bit pattern, matching how a `u8`
        // scalar already renders.
        assert_eq!(to_string(&ModeU8::High).unwrap(), "-56b");
        assert_eq!(to_string(&ModeU16::High).unwrap(), "-25536s");
        assert_eq!(to_string(&ModeU32::High).unwrap(), "-1294967296");
        assert_eq!(to_string(&ModeI64::High).unwrap(), "-9000000000000l");
    }

    /// A literal written by hand — with or without the suffix — parses, so an
    /// SNBT document does not have to come from nbtx.
    #[test]
    fn a_hand_written_literal_parses() {
        assert_eq!(from_string::<ModeU8>("-56b").unwrap(), ModeU8::High);
        assert_eq!(from_string::<ModeU8>("-56").unwrap(), ModeU8::High);
        assert_eq!(
            from_string::<ModeStr>("\"creative\"").unwrap(),
            ModeStr::Creative
        );
        assert_eq!(
            from_string::<ModeStr>("creative").unwrap(),
            ModeStr::Creative
        );
        assert!(
            from_string::<ModeU8>("7b").is_err(),
            "no variant has discriminant 7"
        );
    }

    /// A struct of enums renders as an ordinary compound.
    #[test]
    fn an_enum_field_of_a_struct_roundtrips() {
        let original = settings();
        let text = to_string(&original).unwrap();
        assert_eq!(
            text, "{mode:\"creative\",difficulty:-56b,seed:-9000000000000l}",
            "each field renders as the literal of its declared mode"
        );
        assert_eq!(from_string::<Settings>(&text).unwrap(), original);
    }

    /// A missing attribute is an error on the textual path too, in both
    /// directions.
    #[test]
    fn an_enum_without_the_attribute_is_refused() {
        assert!(matches!(
            to_string(&Undeclared::Survival),
            Err(Error::MissingVariantAs(_))
        ));
        assert!(matches!(
            from_string::<Undeclared>("\"Survival\""),
            Err(Error::MissingVariantAs(_))
        ));
    }

    /// An out-of-range discriminant is refused here as well: the three codecs
    /// agree about which values are representable at all.
    #[test]
    fn an_out_of_range_discriminant_is_refused() {
        #[derive(Facet, Debug)]
        #[facet(nbtx::variant_as(u8))]
        #[repr(u16)]
        enum TooWide {
            Big = 300,
        }
        assert!(matches!(
            to_string(&TooWide::Big),
            Err(Error::DiscriminantOutOfRange(_))
        ));
    }

    /// What the textual codec writes and what the binary one writes describe the
    /// same document.
    #[cfg(feature = "nbt")]
    #[test]
    fn the_textual_and_binary_codecs_agree() {
        macro_rules! check {
            ($ty:ty, $value:expr) => {{
                let original: $ty = $value;
                let from_text: Value = from_string(&to_string(&original).unwrap()).unwrap();
                assert_eq!(
                    from_text,
                    to_value(&original).unwrap(),
                    "{} disagrees between the two paths",
                    stringify!($ty)
                );
            }};
        }
        for_each_mode!(check);
    }
}
