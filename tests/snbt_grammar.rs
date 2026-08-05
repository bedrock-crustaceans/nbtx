//! The SNBT lexical grammar: number literals, quoting, whitespace and the
//! typed-array forms.
//!
//! `snbt.rs` covers the codec's behaviour (round-trips, depth limits, duplicate
//! keys); this file covers what the *parser accepts and what it makes of it*.
//! SNBT is the format users type by hand into commands and config files, so
//! every accepted spelling is a compatibility promise and every rejected one is
//! an error message someone will read.
//!
//! Where nbtx diverges from vanilla Java-Edition SNBT the divergence is asserted
//! explicitly, so it is a recorded decision rather than an accident.

#![cfg(feature = "snbt")]

use bstr::BString;
use nbtx::{Compound, Value, from_string, to_string};

/// Parses `{a:<literal>}` and returns the value bound to `a`.
#[track_caller]
fn parse_literal(literal: &str) -> Value {
    let src = format!("{{a:{literal}}}");
    let v: Value =
        from_string(&src).unwrap_or_else(|e| panic!("{literal:?} should parse but failed: {e}"));
    v.as_compound()
        .unwrap()
        .get(&BString::from("a"))
        .unwrap()
        .clone()
}

fn get<'a>(v: &'a Value, key: &str) -> &'a Value {
    v.as_compound().unwrap().get(&BString::from(key)).unwrap()
}

// --- number literals ------------------------------------------------------

/// Each integer suffix selects its tag, in either case. Minecraft's own parser
/// accepts both cases, so a document written by a command block must parse here.
#[test]
fn integer_suffixes_are_case_insensitive() {
    assert_eq!(parse_literal("1b"), Value::Byte(1));
    assert_eq!(parse_literal("1B"), Value::Byte(1));
    assert_eq!(parse_literal("1s"), Value::Short(1));
    assert_eq!(parse_literal("1S"), Value::Short(1));
    assert_eq!(parse_literal("1l"), Value::Long(1));
    assert_eq!(parse_literal("1L"), Value::Long(1));
}

/// The same for the two floating-point suffixes.
#[test]
fn float_suffixes_are_case_insensitive() {
    assert_eq!(parse_literal("1.5f"), Value::Float(1.5));
    assert_eq!(parse_literal("1.5F"), Value::Float(1.5));
    assert_eq!(parse_literal("1.5d"), Value::Double(1.5));
    assert_eq!(parse_literal("1.5D"), Value::Double(1.5));
    // A suffix works on a whole number too, which is how an integral float is
    // written without a decimal point.
    assert_eq!(parse_literal("2f"), Value::Float(2.0));
    assert_eq!(parse_literal("2D"), Value::Double(2.0));
}

/// With no suffix the tag comes from the literal's *shape*: an integer is an
/// `Int` (the widest tag with no suffix of its own), a decimal is a `Double`.
#[test]
fn an_unsuffixed_literal_takes_its_tag_from_its_shape() {
    assert_eq!(parse_literal("42"), Value::Int(42));
    assert_eq!(parse_literal("-42"), Value::Int(-42));
    assert_eq!(parse_literal("42.0"), Value::Double(42.0));
    assert_eq!(parse_literal("-0.5"), Value::Double(-0.5));
}

/// Both abbreviated decimal forms parse — `1.` and `.5` are what a human types
/// in a hurry, and Rust's own float parser accepts them.
#[test]
fn abbreviated_decimal_forms_parse_as_doubles() {
    assert_eq!(parse_literal("1."), Value::Double(1.0));
    assert_eq!(parse_literal(".5"), Value::Double(0.5));
    assert_eq!(parse_literal("-.5"), Value::Double(-0.5));
}

/// Scientific notation is accepted, in either case, and lands on `Double` when
/// unsuffixed — it cannot be an `Int`, so the integer branch must not claim it.
#[test]
fn scientific_notation_parses_as_a_double() {
    assert_eq!(parse_literal("1e3"), Value::Double(1000.0));
    assert_eq!(parse_literal("1E3"), Value::Double(1000.0));
    assert_eq!(parse_literal("1.5e-3"), Value::Double(0.0015));
    assert_eq!(parse_literal("-2E2"), Value::Double(-200.0));
}

/// A suffix still wins over the shape, so scientific notation can name a
/// `Float`. The suffix is stripped before the digits are parsed, which is the
/// part most easily broken by an off-by-one.
#[test]
fn scientific_notation_respects_an_explicit_suffix() {
    assert_eq!(parse_literal("1e3f"), Value::Float(1000.0));
    assert_eq!(parse_literal("1e3d"), Value::Double(1000.0));
    assert_eq!(parse_literal("1.5E2F"), Value::Float(150.0));
}

/// A leading `+` and leading zeros are accepted, matching `Integer.parseInt`,
/// which is what vanilla's parser falls back on.
#[test]
fn leading_signs_and_zeros_are_accepted() {
    assert_eq!(parse_literal("+5"), Value::Int(5));
    assert_eq!(parse_literal("007"), Value::Int(7));
    assert_eq!(parse_literal("+7b"), Value::Byte(7));
    assert_eq!(parse_literal("-007"), Value::Int(-7));
}

/// An unsuffixed integer too large for an `i32` is **not** an error: it falls
/// back to an unquoted string, exactly as vanilla does (its `TagParser` catches
/// `NumberFormatException` and keeps the token as text). Rejecting it would
/// break documents Minecraft itself accepts.
#[test]
fn an_integer_too_large_for_int_becomes_a_string() {
    assert_eq!(
        parse_literal("3000000000"),
        Value::String(BString::from("3000000000"))
    );
    // Suffixing it makes the intent explicit, and then it *is* a number.
    assert_eq!(parse_literal("3000000000l"), Value::Long(3_000_000_000));
}

/// A *suffixed* literal outside its type's range is an error, though: the suffix
/// states the intent, so silently reinterpreting it as text would hide a typo.
#[test]
fn a_suffixed_literal_out_of_range_is_an_error() {
    for literal in ["128b", "-129b", "32768s", "-32769s", "9223372036854775808l"] {
        let src = format!("{{a:{literal}}}");
        let res: Result<Value, _> = from_string(&src);
        assert!(
            matches!(res, Err(nbtx::Error::ParseIntError(_))),
            "{literal} must be a range error, got {res:?}"
        );
    }
}

/// Number spellings the grammar does *not* recognise degrade to unquoted
/// strings rather than erroring — the same fallback as the oversized integer,
/// and the reason `minecraft:stone` can be written bare.
#[test]
fn unrecognised_number_spellings_become_strings() {
    for literal in ["0x10", "1_000", "0b101", "Infinity", "1/2"] {
        assert_eq!(
            parse_literal(literal),
            Value::String(BString::from(literal)),
            "{literal} should fall back to a string"
        );
    }
}

/// A one-character token is never read as a bare suffix: `b` is the string "b",
/// not a `Byte` with no digits.
#[test]
fn a_lone_suffix_letter_is_a_string() {
    for literal in ["b", "s", "l", "f", "d", "B", "L"] {
        assert_eq!(
            parse_literal(literal),
            Value::String(BString::from(literal))
        );
    }
}

/// nbtx follows vanilla and reads bare `true`/`false` as `Byte(1)`/`Byte(0)`.
/// The match is case-*sensitive*, so `True` remains a string — which matters,
/// because a mixed-case token is far more likely to be a block state than a
/// boolean.
#[test]
fn bare_booleans_are_bytes_and_the_match_is_case_sensitive() {
    assert_eq!(parse_literal("true"), Value::Byte(1));
    assert_eq!(parse_literal("false"), Value::Byte(0));
    assert_eq!(parse_literal("True"), Value::String(BString::from("True")));
    assert_eq!(parse_literal("TRUE"), Value::String(BString::from("TRUE")));
    // Quoting forces the string reading even for the lowercase spelling.
    assert_eq!(
        parse_literal("\"true\""),
        Value::String(BString::from("true"))
    );
}

/// A quoted number is text, not a number: quoting is the only way to write a
/// string that looks numeric.
#[test]
fn quoting_a_number_keeps_it_a_string() {
    assert_eq!(parse_literal("\"42\""), Value::String(BString::from("42")));
    assert_eq!(
        parse_literal("\"1.5f\""),
        Value::String(BString::from("1.5f"))
    );
}

// --- strings and quoting --------------------------------------------------

/// The two characters that would otherwise terminate or corrupt a quoted literal
/// are escaped on write and un-escaped on read, so both survive a round-trip
/// inside a value *and* inside a key.
#[test]
fn quotes_and_backslashes_survive_in_values_and_keys() {
    let doc = Value::Compound(Compound::from([
        (BString::from("q\"k"), Value::String(BString::from("v\"1"))),
        (BString::from("b\\k"), Value::String(BString::from("v\\2"))),
    ]));
    let text = to_string(&doc).unwrap();
    let back: Value = from_string(&text).unwrap();
    assert_eq!(back, doc, "rendered as {text}");
}

/// An unknown escape keeps the escaped character literally, so a stray `\n` in
/// hand-written SNBT yields the letter `n` rather than an error. (Vanilla only
/// defines `\"` and `\\` too.)
#[test]
fn an_unknown_escape_yields_the_escaped_character() {
    assert_eq!(
        parse_literal(r#""a\nb""#),
        Value::String(BString::from("anb"))
    );
    assert_eq!(parse_literal(r#""\t""#), Value::String(BString::from("t")));
}

/// Non-ASCII text is ordinary content: the parser walks characters, not bytes,
/// so multi-byte sequences must not be split.
#[test]
fn unicode_survives_inside_quoted_strings_and_keys() {
    let doc = Value::Compound(Compound::from([
        (
            BString::from("ключ"),
            Value::String(BString::from("значение")),
        ),
        (
            BString::from("絵"),
            Value::String(BString::from("🧊 emoji")),
        ),
    ]));
    let text = to_string(&doc).unwrap();
    let back: Value = from_string(&text).unwrap();
    assert_eq!(back, doc);
    assert_eq!(
        get(&back, "絵").as_string().unwrap(),
        &BString::from("🧊 emoji")
    );
}

/// An unquoted token stops only at a structural delimiter, so — as long as its
/// last character is not a type-suffix letter (see the two tests below) — it may
/// hold non-ASCII characters and punctuation that vanilla's stricter
/// `[0-9A-Za-z_.+-]` unquoted charset would reject.
#[test]
fn an_unquoted_token_stops_only_at_a_delimiter() {
    assert_eq!(parse_literal("café"), Value::String(BString::from("café")));
    assert_eq!(parse_literal("a/c"), Value::String(BString::from("a/c")));
    assert_eq!(parse_literal("a!c"), Value::String(BString::from("a!c")));
    assert_eq!(
        parse_literal("stone"),
        Value::String(BString::from("stone"))
    );

    // A colon *is* a delimiter (it separates a key from its value), so a
    // namespaced id has to be quoted — exactly as it is in vanilla, whose
    // unquoted charset also excludes `:`.
    let res: Result<Value, _> = from_string("{a:minecraft:stone}");
    assert!(
        res.is_err(),
        "a bare namespaced id must be rejected: {res:?}"
    );
    assert_eq!(
        parse_literal("\"minecraft:stone\""),
        Value::String(BString::from("minecraft:stone"))
    );
}

/// An unquoted token whose **last character happens to be a type-suffix letter**
/// (`b`/`s`/`l`/`f`/`d`, either case) is a string, not a malformed number.
///
/// `src/snbt/de.rs::token_to_value` only enters the number grammar when the part
/// before the suffix looks like a number, exactly as vanilla's `TagParser` only
/// enters it when one of its numeric regexes matches. Real Minecraft data would
/// otherwise hit this constantly — `sand`, `gold`, `red`, `glass`, `oak_slab`
/// and `diamond_sword` were all rejected before the fallback existed.
#[test]
fn an_unquoted_string_ending_in_a_suffix_letter_should_be_a_string() {
    for token in ["sand", "gold", "red", "glass", "oak_slab", "diamond_sword"] {
        assert_eq!(
            parse_literal(token),
            Value::String(BString::from(token)),
            "{token} must fall back to a string, as vanilla SNBT does"
        );
    }
}

/// The fallback must stop exactly where the number grammar begins, or it would
/// swallow the typos `a_suffixed_literal_out_of_range_is_an_error` relies on
/// being reported. A token is a string when the part before the suffix does not
/// *look* like a number; it is a number (and so a possible parse error) as soon
/// as it does.
#[test]
fn the_string_fallback_does_not_swallow_malformed_numbers() {
    // Looks like nothing numeric → string, whatever the trailing letter.
    for token in ["S", "ab", "e5f", "x1d", "_2s", "a-1b"] {
        assert_eq!(
            parse_literal(token),
            Value::String(BString::from(token)),
            "{token} must be a string"
        );
    }

    // Starts like a number → still committed to the number grammar, so a
    // malformed or out-of-range literal is reported rather than silently kept
    // as text.
    for token in [
        "128b",
        "1.5b",
        "1.2.3f",
        "-129b",
        "+9999999999999999999999l",
    ] {
        let res: Result<Value, _> = from_string(format!("{{a:{token}}}"));
        assert!(
            matches!(
                res,
                Err(nbtx::Error::ParseIntError(_) | nbtx::Error::ParseFloatError(_))
            ),
            "{token} must stay a number error, got {res:?}"
        );
    }

    // Genuinely suffixed numbers are unaffected, in both cases.
    assert_eq!(parse_literal("7b"), Value::Byte(7));
    assert_eq!(parse_literal("7B"), Value::Byte(7));
    assert_eq!(parse_literal("1.5f"), Value::Float(1.5));
    assert_eq!(parse_literal(".5d"), Value::Double(0.5));

    // And a string that *is* quoted is still a string, including one that would
    // have parsed as a number bare.
    assert_eq!(parse_literal("\"7b\""), Value::String(BString::from("7b")));
}

/// Real block/item ids must survive a full text round-trip, not merely parse:
/// nbtx's own writer quotes every string, so the bare form only ever arrives
/// from hand-written or foreign SNBT.
#[test]
fn barewords_that_end_in_a_suffix_letter_roundtrip() {
    for token in ["sand", "gold", "red", "glass", "oak_slab", "diamond_sword"] {
        let doc = Value::Compound(Compound::from([(
            BString::from("id"),
            Value::String(BString::from(token)),
        )]));
        // Written quoted...
        let text = to_string(&doc).unwrap();
        assert_eq!(text, format!("{{id:\"{token}\"}}"));
        // ...and read back the same from either spelling.
        let from_quoted: Value = from_string(&text).unwrap();
        let from_bare: Value = from_string(format!("{{id:{token}}}")).unwrap();
        assert_eq!(from_quoted, doc);
        assert_eq!(from_bare, doc);
    }
}

/// An empty quoted string is a legal value and a legal key, and both must
/// survive a round-trip — an empty key would be rendered bare (and so lost) if
/// the writer's "is this a simple identifier" test forgot the empty case.
#[test]
fn empty_strings_and_empty_keys_roundtrip() {
    let doc = Value::Compound(Compound::from([
        (BString::from(""), Value::String(BString::from(""))),
        (BString::from("k"), Value::String(BString::from(""))),
    ]));
    let text = to_string(&doc).unwrap();
    assert!(
        text.contains("\"\":"),
        "the empty key must be quoted: {text}"
    );
    let back: Value = from_string(&text).unwrap();
    assert_eq!(back, doc);
}

/// The writer quotes a key only when it has to. Keeping simple keys bare is what
/// makes the output readable, and it is the rule most likely to be widened
/// accidentally into something that no longer round-trips.
#[test]
fn keys_are_quoted_only_when_they_are_not_simple_identifiers() {
    let bare = ["plain", "a1", "a_b", "a.b", "a+b", "a-b", "0"];
    for k in bare {
        let doc = Value::Compound(Compound::from([(BString::from(k), Value::Int(1))]));
        assert_eq!(
            to_string(&doc).unwrap(),
            format!("{{{k}:1}}"),
            "{k} must stay bare"
        );
    }
    let quoted = ["with space", "a:b", "a/b", "a,b", "", "é"];
    for k in quoted {
        let doc = Value::Compound(Compound::from([(BString::from(k), Value::Int(1))]));
        let text = to_string(&doc).unwrap();
        assert!(
            text.starts_with(&format!("{{\"{k}\"")),
            "{k} must be quoted: {text}"
        );
        assert_eq!(from_string::<Value>(&text).unwrap(), doc);
    }
}

/// A `Value::String` holding raw, non-UTF-8 bytes cannot be represented in text.
/// SNBT renders it lossily (U+FFFD per invalid byte) **by design**; this pins
/// that so the loss is a documented property rather than a surprise, and shows
/// the binary codec as the lossless alternative.
#[test]
fn a_non_utf8_string_is_rendered_lossily() {
    let raw = Value::String(BString::from(vec![0xffu8, 0xfe]));
    let text = to_string(&raw).unwrap();
    let back: Value = from_string(&text).unwrap();
    assert_ne!(back, raw, "the raw bytes cannot survive a text round-trip");
    assert_eq!(
        back.as_string().unwrap(),
        &BString::from("\u{fffd}\u{fffd}"),
        "invalid bytes become the replacement character"
    );
}

// --- whitespace and separators --------------------------------------------

/// Whitespace between tokens is insignificant, including tabs and carriage
/// returns, and in every position the grammar allows one.
#[test]
fn whitespace_is_insignificant_everywhere_between_tokens() {
    let spaced = " \t\r\n { \r\n a \t : \n 1 \t , \r b : [ 1b , 2b ] , c : { d : 1l } } \n ";
    let packed = "{a:1,b:[1b,2b],c:{d:1l}}";
    assert_eq!(
        from_string::<Value>(spaced).unwrap(),
        from_string::<Value>(packed).unwrap()
    );
}

/// Whitespace *inside* quotes is content, and whitespace immediately inside a
/// typed-array literal is not.
#[test]
fn whitespace_inside_quotes_is_content_but_inside_arrays_is_not() {
    assert_eq!(
        parse_literal("\"  padded  \""),
        Value::String(BString::from("  padded  "))
    );
    assert_eq!(
        from_string::<Value>("{a:[I; 1 , 2 ]}").unwrap(),
        from_string::<Value>("{a:[I;1,2]}").unwrap()
    );
}

/// A trailing comma is tolerated in every container, which is what makes
/// generated SNBT easy to emit line by line.
#[test]
fn a_trailing_comma_is_tolerated_in_every_container() {
    assert_eq!(
        from_string::<Value>("{a:1,}").unwrap(),
        from_string::<Value>("{a:1}").unwrap()
    );
    assert_eq!(
        from_string::<Value>("{a:[1,2,]}").unwrap(),
        from_string::<Value>("{a:[1,2]}").unwrap()
    );
    assert_eq!(
        from_string::<Value>("{a:[I;1,2,]}").unwrap(),
        from_string::<Value>("{a:[I;1,2]}").unwrap()
    );
    assert_eq!(
        from_string::<Value>("{a:[B;1b,]}").unwrap(),
        from_string::<Value>("{a:[B;1b]}").unwrap()
    );
}

// --- typed arrays ---------------------------------------------------------

/// The typed-array markers are the distinction between `[B;1b]` (a `ByteArray`
/// tag) and `[1b]` (a `List` of bytes), and the element suffixes inside them are
/// optional and case-insensitive.
#[test]
fn typed_array_elements_accept_optional_case_insensitive_suffixes() {
    assert_eq!(
        parse_literal("[B;1b,2B,3]"),
        Value::ByteArray(vec![1, 2, 3])
    );
    assert_eq!(parse_literal("[I;1,2,3]"), Value::IntArray(vec![1, 2, 3]));
    assert_eq!(
        parse_literal("[L;1l,2L,3]"),
        Value::LongArray(vec![1, 2, 3])
    );
    // Without the marker the same digits are a plain list.
    assert_eq!(
        parse_literal("[1b,2b]"),
        Value::List(vec![Value::Byte(1), Value::Byte(2)])
    );
}

/// An empty typed array keeps its tag, unlike an empty plain list — which is the
/// whole point of the marker, and the one case where the marker carries
/// information no element can supply.
#[test]
fn an_empty_typed_array_keeps_its_tag() {
    assert_eq!(parse_literal("[B;]"), Value::ByteArray(vec![]));
    assert_eq!(parse_literal("[I;]"), Value::IntArray(vec![]));
    assert_eq!(parse_literal("[L;]"), Value::LongArray(vec![]));
    assert_eq!(parse_literal("[]"), Value::List(vec![]));
}

/// A byte-array element is written signed but stored unsigned, so the negative
/// spelling and the high-bit spelling must denote the same byte. This is how
/// `0xff` appears in Minecraft's own output (`-1b`).
#[test]
fn byte_array_elements_are_signed_on_the_page_and_unsigned_in_memory() {
    assert_eq!(parse_literal("[B;-1b]"), Value::ByteArray(vec![0xff]));
    assert_eq!(parse_literal("[B;-128b]"), Value::ByteArray(vec![0x80]));
    assert_eq!(parse_literal("[B;127b]"), Value::ByteArray(vec![0x7f]));
    // ...and the writer renders them back the same way.
    assert_eq!(to_string(&Value::ByteArray(vec![0xff])).unwrap(), "[B;-1b]");
}

/// A typed array holds scalars and cannot recurse, so it is a *leaf* for the
/// depth guard. Placing one at the very bottom of a `MAX_DEPTH`-deep document
/// must therefore still parse — if arrays counted, this would be one over.
#[test]
fn a_typed_array_does_not_count_towards_the_depth_limit() {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let depth = nbtx::MAX_DEPTH;
            let input = format!("{}[I;1,2]{}", "{a:".repeat(depth), "}".repeat(depth));
            let res: Result<Value, _> = from_string(&input);
            assert!(
                res.is_ok(),
                "a typed array at MAX_DEPTH must parse: {res:?}"
            );
        })
        .unwrap()
        .join()
        .unwrap();
}

// --- float rendering ------------------------------------------------------

/// The special float values must survive the text round-trip: Rust renders them
/// as `NaN`/`inf`, and the suffix-stripping parser feeds those straight back to
/// `f32::from_str`, which accepts them.
#[test]
fn float_special_values_survive_the_text_roundtrip() {
    for f in [f32::INFINITY, f32::NEG_INFINITY, -0.0f32, f32::MIN_POSITIVE] {
        let v = Value::Float(f);
        let back: Value = from_string(to_string(&v).unwrap()).unwrap();
        assert_eq!(
            back.as_float().unwrap().to_bits(),
            f.to_bits(),
            "f32 {f} did not survive as {}",
            to_string(&v).unwrap()
        );
    }
    for d in [f64::INFINITY, f64::NEG_INFINITY, -0.0f64, f64::MIN_POSITIVE] {
        let v = Value::Double(d);
        let back: Value = from_string(to_string(&v).unwrap()).unwrap();
        assert_eq!(back.as_double().unwrap().to_bits(), d.to_bits(), "f64 {d}");
    }
}

/// A NaN survives *as a NaN*, but its payload bits do not: the text `NaN` has
/// nowhere to carry them. That is inherent to a textual format, and it is the
/// reason the property tests compare SNBT round-trips with a NaN-tolerant
/// predicate while the binary ones compare bit patterns.
#[test]
fn nan_payload_bits_do_not_survive_snbt() {
    let original = f32::from_bits(0x7fc0_1234);
    assert!(original.is_nan());
    let text = to_string(&Value::Float(original)).unwrap();
    assert_eq!(text, "NaNf");
    let back: Value = from_string(&text).unwrap();
    let got = *back.as_float().unwrap();
    assert!(got.is_nan(), "it is still a NaN");
    assert_ne!(got.to_bits(), original.to_bits(), "but not the same one");
}

/// Ordinary decimals must round-trip exactly, not merely closely: Rust's `f32`/
/// `f64` `Display` emits the shortest representation that parses back to the
/// same value, so no precision is lost through the text form.
#[test]
fn ordinary_decimals_roundtrip_without_precision_loss() {
    for f in [0.1f32, 1.0 / 3.0, f32::MAX, f32::MIN, 1e-30, 123.456] {
        let back: Value = from_string(to_string(&Value::Float(f)).unwrap()).unwrap();
        assert_eq!(back.as_float().unwrap().to_bits(), f.to_bits(), "f32 {f}");
    }
    for d in [0.1f64, 1.0 / 3.0, f64::MAX, f64::MIN, 1e-300, 123.456] {
        let back: Value = from_string(to_string(&Value::Double(d)).unwrap()).unwrap();
        assert_eq!(back.as_double().unwrap().to_bits(), d.to_bits(), "f64 {d}");
    }
}

/// The renderer always writes a suffix for every tag but `Int`, so the tag can
/// be recovered from the text alone. Without it a `Long(1)` would come back as
/// an `Int(1)`.
#[test]
fn every_scalar_tag_but_int_renders_with_its_suffix() {
    assert_eq!(to_string(&Value::Byte(1)).unwrap(), "1b");
    assert_eq!(to_string(&Value::Short(1)).unwrap(), "1s");
    assert_eq!(to_string(&Value::Int(1)).unwrap(), "1");
    assert_eq!(to_string(&Value::Long(1)).unwrap(), "1l");
    assert_eq!(to_string(&Value::Float(1.0)).unwrap(), "1f");
    assert_eq!(to_string(&Value::Double(1.0)).unwrap(), "1d");
    // ...and each of those texts reads back as the same tag.
    for v in [
        Value::Byte(1),
        Value::Short(1),
        Value::Int(1),
        Value::Long(1),
        Value::Float(1.0),
        Value::Double(1.0),
    ] {
        let back: Value = from_string(to_string(&v).unwrap()).unwrap();
        assert_eq!(back.discriminant(), v.discriminant(), "{v:?} lost its tag");
    }
}

// --- structural forms -----------------------------------------------------

/// The parser accepts any root value, mirroring the binary codec's "any root tag
/// but `TAG_End`" rule, so a bare list or scalar is a complete document.
#[test]
fn any_value_is_a_legal_root() {
    assert_eq!(from_string::<Value>("5").unwrap(), Value::Int(5));
    assert_eq!(from_string::<Value>("5b").unwrap(), Value::Byte(5));
    assert_eq!(
        from_string::<Value>("\"hi\"").unwrap(),
        Value::String(BString::from("hi"))
    );
    assert_eq!(
        from_string::<Value>("[1,2]").unwrap(),
        Value::List(vec![Value::Int(1), Value::Int(2)])
    );
    assert_eq!(
        from_string::<Value>("[B;1b]").unwrap(),
        Value::ByteArray(vec![1])
    );
}

/// A root value also decodes straight into a concrete Rust type, not only into a
/// `Value` — the textual counterpart of the binary codec's root-type symmetry.
#[test]
fn a_root_value_decodes_into_a_concrete_rust_type() {
    assert_eq!(from_string::<i32>("5").unwrap(), 5);
    assert_eq!(from_string::<String>("\"hi\"").unwrap(), "hi");
    assert_eq!(from_string::<Vec<i32>>("[I;1,2]").unwrap(), vec![1, 2]);
    assert_eq!(from_string::<Vec<u8>>("[B;1b,2b]").unwrap(), vec![1, 2]);
}

/// Lists may nest heterogeneously in the *parser* — the element-type constraint
/// belongs to the binary wire format, not the grammar — but the nesting itself
/// must be structurally correct.
#[test]
fn nested_containers_parse_to_the_matching_shape() {
    let v: Value = from_string("{a:[{b:[1,2]},{b:[3]}]}").unwrap();
    let outer = get(&v, "a").as_list().unwrap();
    assert_eq!(outer.len(), 2);
    assert_eq!(get(&outer[0], "b").as_list().unwrap().len(), 2);
    assert_eq!(get(&outer[1], "b").as_list().unwrap().len(), 1);
}

/// Malformed structure is an error rather than a partial parse — every one of
/// these stops the parser rather than silently producing a truncated document.
#[test]
fn structurally_malformed_input_is_rejected() {
    for input in [
        "{a:1,,b:2}", // doubled separator
        "{:1}",       // missing key
        "{a:1}}",     // ...only the *inner* form; see the trailing-input gap below
        "[I;1,2",     // unterminated typed array
        "{a:[}",      // mismatched brackets
        "[B;x]",      // non-numeric typed-array element
    ] {
        let res: Result<Value, _> = from_string(input);
        if input == "{a:1}}" {
            // KNOWN GAP: trailing input after the first complete value is
            // ignored (see `snbt::trailing_characters_rejected`), so this one
            // parses. Asserted here so the gap stays visible in the grammar
            // tests too, rather than looking like an oversight.
            assert!(res.is_ok(), "{input:?}: the trailing-input gap changed");
            continue;
        }
        assert!(res.is_err(), "{input:?} must be rejected, got {res:?}");
    }
}
