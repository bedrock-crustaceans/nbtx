//! Integration tests for stringified NBT (SNBT).

#![cfg(feature = "snbt")]

use bstr::BString;
use facet::Facet;
use nbtx::Compound;
use nbtx::{Value, ValueList, from_string, to_string};

const WHITESPACED_ALL: &str = r#"
    {
        float: 42f,
        "double 1": 42d,
        byte_array: [1b, 2b, 3b],
        compound: {
            name: "Compound 3"
        },
        list: [
            {
                name: "Compound 1"
            },
            {
                name: "Compound 2"
            }
        ],
        string: "Hello, World!",
        long: 42l,
        byte: 42b,
        short: 42s,
        int: 42,
    }
"#;

#[derive(Facet, Debug, PartialEq)]
struct Data {
    byte: i8,
    tuple: Vec<i32>,
}

#[test]
fn simple_snbt_roundtrip() {
    let value = Data {
        byte: 7,
        tuple: vec![1; 5],
    };

    let ser = to_string(&value).unwrap();
    // Vec<i32> renders as an IntArray literal.
    assert_eq!(ser, "{byte:7b,tuple:[I;1,1,1,1,1]}");

    let val: Data = from_string(&ser).unwrap();
    assert_eq!(val, value);
}

/// Exact-output test: a `String` field renders as a quoted string, a
/// `Vec<u8>` field renders as a `[B;..]` byte-array literal.
#[test]
fn snbt_string_and_byte_array_exact_output() {
    #[derive(Facet)]
    struct Holder {
        name: String,
        data: Vec<u8>,
    }

    let holder = Holder {
        name: "Hello, World!".to_owned(),
        data: vec![1, 2, 0xff],
    };

    let out = to_string(&holder).unwrap();
    assert_eq!(out, r#"{name:"Hello, World!",data:[B;1b,2b,-1b]}"#);

    // The literal round-trips through a `Value` as a distinct ByteArray.
    let value: Value = from_string(&out).unwrap();
    let compound = value.as_compound().unwrap();
    assert_eq!(
        compound.get(&BString::from("name")).unwrap(),
        &Value::String("Hello, World!".into())
    );
    assert_eq!(
        compound.get(&BString::from("data")).unwrap(),
        &Value::ByteArray(vec![1, 2, 0xff])
    );
}

/// Quotes and backslashes inside a string are escaped on write and un-escaped on
/// read, so an SNBT round-trip is exact.
#[test]
fn snbt_string_escaping_roundtrip() {
    let value = Value::String(r#"he said "hi" \ end"#.into());

    let snbt = to_string(&value).unwrap();
    // The quote and backslash are backslash-escaped in the output.
    assert_eq!(snbt, r#""he said \"hi\" \\ end""#);

    let back: Value = from_string(&snbt).unwrap();
    assert_eq!(back, value);
}

/// The parser tolerates newlines, indentation and trailing commas.
#[test]
fn snbt_parse_tolerance() {
    let out: Value = from_string(WHITESPACED_ALL).unwrap();
    let compound = out.as_compound().unwrap();

    assert_eq!(
        compound.get(&BString::from("float")).unwrap(),
        &Value::Float(42.0)
    );
    assert_eq!(
        compound.get(&BString::from("byte")).unwrap(),
        &Value::Byte(42)
    );
    assert_eq!(
        compound.get(&BString::from("short")).unwrap(),
        &Value::Short(42)
    );
    assert_eq!(
        compound.get(&BString::from("long")).unwrap(),
        &Value::Long(42)
    );
    assert_eq!(
        compound.get(&BString::from("int")).unwrap(),
        &Value::Int(42)
    );
    assert_eq!(
        compound.get(&BString::from("double 1")).unwrap(),
        &Value::Double(42.0)
    );
    // `[1b,2b,3b]` (no `B;`) is a plain list of bytes, not a ByteArray.
    assert_eq!(
        compound.get(&BString::from("byte_array")).unwrap(),
        &Value::List(ValueList::Byte(vec![1, 2, 3]))
    );
}

/// A dynamic `Value` with every tag round-trips through SNBT, including the
/// distinct `[B;]`/`[I;]`/`[L;]` typed arrays.
#[test]
fn value_all_tags_roundtrip() {
    let value = Value::Compound(Compound::from([
        ("byte".into(), Value::Byte(-3)),
        ("short".into(), Value::Short(300)),
        ("int".into(), Value::Int(70_000)),
        ("long".into(), Value::Long(5_000_000_000)),
        ("float".into(), Value::Float(1.5)),
        ("double".into(), Value::Double(2.25)),
        ("byte_array".into(), Value::ByteArray(vec![1, 2, 0xff])),
        ("int_array".into(), Value::IntArray(vec![1, -2, 3])),
        ("long_array".into(), Value::LongArray(vec![10, 20, 30])),
        ("string".into(), Value::String("hi".into())),
        ("list".into(), Value::List(ValueList::Int(vec![1, 2]))),
        (
            "nested".into(),
            Value::Compound(Compound::from([("x".into(), Value::Byte(9))])),
        ),
    ]));

    let snbt = to_string(&value).unwrap();
    let back: Value = from_string(&snbt).unwrap();
    assert_eq!(value, back);

    let compound = back.as_compound().unwrap();
    assert!(
        compound
            .get(&BString::from("byte_array"))
            .unwrap()
            .is_byte_array()
    );
    assert!(
        compound
            .get(&BString::from("int_array"))
            .unwrap()
            .is_int_array()
    );
    assert!(
        compound
            .get(&BString::from("long_array"))
            .unwrap()
            .is_long_array()
    );
    assert!(compound.get(&BString::from("list")).unwrap().is_list());
}

/// Malformed SNBT must return `Err`, never panic.
#[test]
fn malformed_snbt_returns_err_not_panic() {
    for input in [
        "{",              // unterminated compound
        "{a:}",           // missing value
        "{a:1",           // missing closing brace
        "[1,2",           // unterminated list
        "\"unterminated", // unterminated string
        "{a 1}",          // missing colon
        "",               // empty input
        "[B;1b,",         // unterminated typed array
    ] {
        let res: Result<Value, _> = from_string(input);
        assert!(res.is_err(), "expected Err for {input:?}");
    }
}

/// A pathologically nested SNBT document must return `Error::MaxDepthExceeded`
/// rather than driving the recursive-descent parser into a stack overflow —
/// which in Rust aborts the whole process (SIGABRT) and cannot be caught.
///
/// 50 000 levels is ~150 KB of text, so this is a very cheap request to send;
/// before the guard existed, a *500*-level document already sufficed to kill the
/// process, i.e. SNBT crashed on documents the binary codec considers legal.
#[test]
fn deep_nesting_is_rejected_on_parse() {
    let input = format!("{}1{}", "{a:".repeat(50_000), "}".repeat(50_000));
    let res: Result<Value, _> = from_string(&input);
    let err = res.expect_err("50k nested compounds must be refused");
    assert!(
        matches!(err, nbtx::Error::MaxDepthExceeded(_)),
        "expected MaxDepthExceeded, got {err:?}"
    );
}

/// The same for nested lists, which recurse through a different function
/// (`parse_list_value`).
#[test]
fn deep_nesting_of_lists_is_rejected_on_parse() {
    let input = format!("{}1{}", "[".repeat(50_000), "]".repeat(50_000));
    let res: Result<Value, _> = from_string(&input);
    let err = res.expect_err("50k nested lists must be refused");
    assert!(
        matches!(err, nbtx::Error::MaxDepthExceeded(_)),
        "expected MaxDepthExceeded, got {err:?}"
    );
}

/// Alternating `{`/`[` exercises the mutual recursion (`parse_compound_value` →
/// `parse_list_value` → …, which dispatch to each other directly rather than
/// through `parse_value`), the deepest-per-byte shape the grammar allows.
#[test]
fn deep_alternating_nesting_is_rejected_on_parse() {
    let input = format!("{}1{}", "{a:[".repeat(25_000), "]}".repeat(25_000));
    let res: Result<Value, _> = from_string(&input);
    let err = res.expect_err("50k alternating containers must be refused");
    assert!(
        matches!(err, nbtx::Error::MaxDepthExceeded(_)),
        "expected MaxDepthExceeded, got {err:?}"
    );
}

/// The bound is exactly `MAX_DEPTH` containers: 512 nested compounds parse, 513
/// do not. Guards against the limit silently drifting (or being made so tight
/// that legitimate documents break).
///
/// Runs on an explicitly-sized thread. Not because 512 frames do not fit in the
/// 2 MiB std default — they do, at ~2.8 KB per level unoptimised, the same order
/// as the binary `Value` path — but because this test walks *right up to* the
/// bound with no margin at all, and libtest's default is exactly that 2 MiB. The
/// rejection tests above hold the real guarantee and run on ordinary test
/// threads.
#[test]
fn max_depth_boundary_is_512_containers() {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let at_limit = format!("{}1{}", "{a:".repeat(512), "}".repeat(512));
            let ok: Result<Value, _> = from_string(&at_limit);
            assert!(ok.is_ok(), "512 nested compounds must parse: {ok:?}");

            let over = format!("{}1{}", "{a:".repeat(513), "}".repeat(513));
            let res: Result<Value, _> = from_string(&over);
            let err = res.expect_err("513 nested compounds must be refused");
            assert!(
                matches!(err, nbtx::Error::MaxDepthExceeded(_)),
                "expected MaxDepthExceeded, got {err:?}"
            );
        })
        .unwrap()
        .join()
        .unwrap();
}

/// The guard must cover the **writer** too, as it does in the binary codec: a
/// deeply nested `Value` built in memory must not overflow the stack on the way
/// out.
#[test]
fn deep_nesting_is_rejected_on_serialize() {
    let mut v = Value::Int(1);
    for _ in 0..2000 {
        v = Value::List(ValueList::try_from(vec![v]).expect("a singleton"));
    }
    let err = to_string(&v).expect_err("serializing a 2000-deep list must be refused");
    assert!(
        matches!(err, nbtx::Error::MaxDepthExceeded(_)),
        "expected MaxDepthExceeded, got {err:?}"
    );

    // The same through a nested compound, which is a separate recursive arm.
    let mut v = Value::Int(1);
    for _ in 0..2000 {
        v = Value::Compound(Compound::from([("a".into(), v)]));
    }
    let err = to_string(&v).expect_err("serializing a 2000-deep compound must be refused");
    assert!(
        matches!(err, nbtx::Error::MaxDepthExceeded(_)),
        "expected MaxDepthExceeded, got {err:?}"
    );
}

/// A *derived* recursive struct must hit the same bound: the guard lives in the
/// facet-reflection parse path (`parse_seq_into`/`parse_struct_into`), not only
/// in the `Value` fast path.
///
/// Runs on an explicitly-sized thread for the same reason as
/// `security_limits::deep_nesting_is_rejected_for_derived_structs`: the
/// reflection path costs roughly 10 KB of stack per container in an unoptimised
/// build (where LLVM gives every temporary its own never-reused slot), so
/// `MAX_DEPTH` containers alone would exhaust libtest's 2 MiB default. The
/// `Value` path — the one that matters for untrusted input — is an order of
/// magnitude cheaper and is covered by the tests above on ordinary threads.
#[test]
fn deep_nesting_is_rejected_for_derived_structs() {
    #[derive(Facet, Debug)]
    struct Nest {
        a: Vec<Nest>,
    }

    // Each level is `{a:[ <next level> ]}`, i.e. two containers per level.
    let input = format!("{}{}", "{a:[".repeat(1000), "]}".repeat(1000));

    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            let res: Result<Nest, _> = from_string(&input);
            let err = res.expect_err("a deeply nested derived struct must error, not overflow");
            assert!(
                matches!(err, nbtx::Error::MaxDepthExceeded(_)),
                "expected MaxDepthExceeded, got {err:?}"
            );
        })
        .unwrap()
        .join()
        .unwrap();
}

#[cfg(feature = "nbt")]
#[test]
fn bigtest_roundtrip_through_snbt() {
    const BIG_TEST_NBT: &[u8] = include_bytes!("fixtures/bigtest.nbt");

    let data: Value = nbtx::from_be_bytes(&mut BIG_TEST_NBT.to_vec().as_slice()).unwrap();

    let snbt = to_string(&data).unwrap();
    let out: Value = from_string(&snbt).unwrap();

    assert_eq!(data, out);
}

/// Looks up a key in a compound `Value`.
fn get<'a>(v: &'a Value, key: &str) -> &'a Value {
    v.as_compound().unwrap().get(&BString::from(key)).unwrap()
}

#[test]
fn incomplete_compound_errors() {
    let res: Result<Value, _> = from_string("{SomeTag:[]");
    assert!(res.is_err());
}

#[test]
fn incomplete_list_errors() {
    let res: Result<Value, _> = from_string("{SomeTag:[");
    assert!(res.is_err());
}

#[test]
fn empty_compound_parses() {
    let v: Value = from_string("{}").unwrap();
    assert!(v.as_compound().unwrap().is_empty());
}

#[test]
fn empty_list_parses() {
    let v: Value = from_string("{TestList:[]}").unwrap();
    assert!(get(&v, "TestList").as_list().unwrap().is_empty());
}

/// Quoting is what lets a key contain characters the bare-token grammar cannot
/// express.
#[test]
fn quoted_keys_parse() {
    let v: Value = from_string("{\"String With Spaces\": 1}").unwrap();
    assert_eq!(get(&v, "String With Spaces"), &Value::Int(1));
}

/// Inside quotes, leading and trailing spaces are content and must be preserved
/// verbatim — only whitespace *between* tokens is insignificant.
#[test]
fn quoted_values_keep_surrounding_spaces() {
    let v: Value = from_string("{TestString:\"  TEST  minecraft:stone  \"}").unwrap();
    assert_eq!(
        get(&v, "TestString"),
        &Value::String("  TEST  minecraft:stone  ".into())
    );
}

/// A duplicated key keeps its **first** value, exactly as the binary codec does,
/// so the two codecs agree on the same document.
#[test]
fn duplicate_snbt_keys_keep_first() {
    let v: Value = from_string("{Test:hi,Test:yo}").unwrap();
    assert_eq!(
        get(&v, "Test"),
        &Value::String("hi".into()),
        "SNBT must keep the first duplicate, like the binary codec"
    );
    assert_eq!(v.as_compound().unwrap().len(), 1);
}

/// The typed-array literals `[B;..]`/`[I;..]`/`[L;..]` parse to the distinct
/// ByteArray/IntArray/LongArray tags, not to plain lists.
#[test]
fn typed_array_literals_parse() {
    let v: Value = from_string("{ba:[B;1b,2b],ia:[I;1,2],la:[L;1l,2l]}").unwrap();
    assert!(get(&v, "ba").is_byte_array());
    assert!(get(&v, "ia").is_int_array());
    assert!(get(&v, "la").is_long_array());
}

/// SNBT infers a tag from the literal's suffix: a bare integer is an Int, a bare
/// decimal is a Double, and each one-letter suffix names its tag.
#[test]
fn number_suffix_inference() {
    let v: Value = from_string("{a:1,b:1b,c:1s,d:1l,e:1.5,f:1.5f,g:1.5d}").unwrap();
    assert_eq!(get(&v, "a"), &Value::Int(1));
    assert_eq!(get(&v, "b"), &Value::Byte(1));
    assert_eq!(get(&v, "c"), &Value::Short(1));
    assert_eq!(get(&v, "d"), &Value::Long(1));
    assert_eq!(get(&v, "e"), &Value::Double(1.5));
    assert_eq!(get(&v, "f"), &Value::Float(1.5));
    assert_eq!(get(&v, "g"), &Value::Double(1.5));
}

/// KNOWN GAP: `from_string` parses one self-describing value and stops, so
/// anything after it is silently ignored rather than reported as a syntax error.
#[test]
#[ignore = "KNOWN GAP: the SNBT parser ignores trailing characters after the first value"]
fn trailing_characters_rejected() {
    let res: Result<Value, _> = from_string("{SomeTag:[]}}");
    assert!(res.is_err(), "trailing characters must be an error");
}

/// KNOWN GAP (deliberate): `from_string` accepts any root value, mirroring the
/// binary codec, which also accepts any root tag. Parsers that require a
/// compound root would reject this.
#[test]
#[ignore = "BY DESIGN: the SNBT parser accepts a non-compound (e.g. list) root, like the binary codec"]
fn non_compound_root_rejected() {
    let res: Result<Value, _> = from_string("[1,2,3]");
    assert!(res.is_err(), "a compound root is required");
}

/// A list may hold only one element type, so the parser refuses a mixture
/// outright — as vanilla Minecraft's own parser does. The text is not
/// representable, so accepting it would only defer the failure to encode time.
#[test]
fn mixed_list_rejected() {
    let res: Result<Value, _> = from_string("{TestList:[1f, string2, 3b]}");
    assert!(res.is_err(), "heterogeneous lists must be rejected");
    assert!(matches!(res, Err(nbtx::Error::HeterogeneousList { .. })));
}

/// Only the *outer* element type has to agree. A list of lists is homogeneous
/// as long as every element is a list, however differently the inner ones are
/// typed — and an inner `[]` is the untyped empty list.
#[test]
fn a_list_of_differently_typed_inner_lists_is_accepted() {
    let v: Value = from_string(r#"{l:[[1b],["x"],[]]}"#).unwrap();
    assert_eq!(
        get(&v, "l"),
        &Value::List(ValueList::List(vec![
            ValueList::Byte(vec![1]),
            ValueList::String(vec![bstr::BString::from("x")]),
            ValueList::End,
        ]))
    );
}

/// KNOWN DIVERGENCE: nbtx follows vanilla Java SNBT and parses bare
/// `true`/`false` as `Byte(1)`/`Byte(0)`. Parsers that only recognise numeric
/// literals treat them as unquoted strings instead.
#[test]
#[ignore = "BY DESIGN: bare true/false parse as Byte, following vanilla SNBT rather than as strings"]
fn bare_true_false_is_string() {
    let v: Value = from_string("{a:true}").unwrap();
    assert_eq!(
        get(&v, "a"),
        &Value::String("true".into()),
        "bare true/false as strings"
    );
}
