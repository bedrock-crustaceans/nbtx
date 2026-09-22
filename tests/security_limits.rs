//! The hard limits that keep decoding untrusted NBT safe.
//!
//! Three bounds, each with a technical reason for its value:
//!
//! * [`nbtx::MAX_DEPTH`] (512 nested containers) — the decoders and encoders are
//!   recursive, and a Rust stack overflow **aborts the process** (SIGABRT)
//!   rather than unwinding into a catchable error. Without a bound, a few
//!   kilobytes of nested `TAG_List`s kill any process that parses them. 512 is
//!   far past anything a real document needs while leaving the recursion
//!   comfortably inside a default 2 MiB thread stack.
//! * [`nbtx::MAX_STRING_LEN`] (32767 = `i16::MAX`) — the big/little-endian
//!   variants prefix strings with a `u16`, so a longer string cannot be
//!   represented; writing one would wrap the prefix and emit a stream that
//!   silently decodes as something else. The varint variant has no such natural
//!   ceiling, so it is checked explicitly, before any allocation.
//! * varint width (5 bytes for 32-bit, 10 for 64-bit) — `ceil(32/7)` and
//!   `ceil(64/7)`. A longer varint shifts past the width of the target type,
//!   which panics in debug builds and silently decodes a wrong value in release
//!   builds; it is rejected instead.
//!
//! Every limit is enforced on **write** as well as read, so an oversized value
//! built in memory fails loudly instead of producing a stream nothing can parse.

#![cfg(feature = "nbt")]

use bstr::BString;
use nbtx::{
    Compound, Value, from_be_bytes, from_le_bytes, from_varint_bytes, to_be_bytes, to_le_bytes,
    to_varint_bytes,
};

fn hex(s: &str) -> Vec<u8> {
    let packed: String = s.split_whitespace().collect();
    assert!(packed.len().is_multiple_of(2), "odd-length hex: {s}");
    (0..packed.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&packed[i..i + 2], 16).unwrap())
        .collect()
}

fn compound(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Compound(
        entries
            .into_iter()
            .map(|(k, v)| (BString::from(k), v))
            .collect::<Compound>(),
    )
}

fn get<'a>(v: &'a Value, key: &str) -> &'a Value {
    v.as_compound().unwrap().get(&BString::from(key)).unwrap()
}

/// Hand-assembles big-endian documents, so tests can craft inputs the encoder
/// would never produce.
mod be {
    pub fn str_payload(out: &mut Vec<u8>, s: &[u8]) {
        out.extend_from_slice(&(s.len() as u16).to_be_bytes());
        out.extend_from_slice(s);
    }
    /// Begins a root compound with an empty name.
    pub fn root_compound() -> Vec<u8> {
        let mut v = vec![10u8];
        str_payload(&mut v, b"");
        v
    }
    pub fn entry_header(out: &mut Vec<u8>, tag: u8, key: &[u8]) {
        out.push(tag);
        str_payload(out, key);
    }
    pub fn end(out: &mut Vec<u8>) {
        out.push(0);
    }
}

// --- varint width ---------------------------------------------------------

/// A 32-bit varint may occupy at most 5 bytes (`ceil(32 / 7)`); a sixth
/// continuation byte would shift past the width of an `i32`.
#[test]
fn overlong_varint32_is_rejected() {
    let bytes = hex(concat!("0a00", "0301", "61", "ffffffffff7f", "00"));
    assert!(
        from_varint_bytes::<Value>(&mut bytes.as_slice()).is_err(),
        "a varint32 that does not terminate within 5 bytes must be rejected"
    );
}

/// The 64-bit equivalent: at most 10 bytes (`ceil(64 / 7)`).
#[test]
fn overlong_varint64_is_rejected() {
    let bytes = hex(concat!(
        "0a00",
        "0401",
        "61",
        "ffffffffffffffffffff7f",
        "00"
    ));
    assert!(
        from_varint_bytes::<Value>(&mut bytes.as_slice()).is_err(),
        "a varint64 that does not terminate within 10 bytes must be rejected"
    );
}

/// An overlong varint must surface as the dedicated `Error::InvalidVarint`, not
/// as a generic EOF — the distinction matters because EOF is recoverable in a
/// streaming caller while malformed framing is not. Covers the Int (varint32),
/// Long (varint64) and String/array length-prefix (varuint32) read paths.
#[test]
fn overlong_varints_report_invalid_varint_on_every_path() {
    let cases: [(&str, &str); 4] = [
        // Int "a": 6-byte varint32.
        ("Int", concat!("0a00", "0301", "61", "ffffffffff7f", "00")),
        // Long "a": 11-byte varint64.
        (
            "Long",
            concat!("0a00", "0401", "61", "ffffffffffffffffffff7f", "00"),
        ),
        // String "a": 6-byte varuint32 length prefix.
        (
            "String length",
            concat!("0a00", "0801", "61", "ffffffffff7f"),
        ),
        // ByteArray "a": 6-byte varint32 length prefix.
        (
            "ByteArray length",
            concat!("0a00", "0701", "61", "ffffffffff7f"),
        ),
    ];
    for (label, h) in cases {
        let bytes = hex(h);
        let err = from_varint_bytes::<Value>(&mut bytes.as_slice())
            .expect_err(&format!("{label}: an overlong varint must be rejected"));
        assert!(
            matches!(err, nbtx::Error::InvalidVarint(_)),
            "{label}: expected InvalidVarint, got {err:?}"
        );
    }
}

/// A varint that is exactly at its maximum width is still valid — the bound
/// rejects *overlong* varints, not maximal ones. `i32::MIN`/`i64::MIN` encode to
/// a full 5/10 bytes, so this pins the boundary from the accepting side.
#[test]
fn maximum_width_varints_are_accepted() {
    for v in [
        Value::Int(i32::MIN),
        Value::Int(i32::MAX),
        Value::Long(i64::MIN),
        Value::Long(i64::MAX),
    ] {
        let doc = compound([("a", v.clone())]);
        let bytes = to_varint_bytes(&doc).unwrap();
        assert_eq!(
            from_varint_bytes::<Value>(&mut bytes.as_slice()).unwrap(),
            doc
        );
    }
    // i32::MIN is 5 varint bytes, i64::MIN is 10 — i.e. exactly at the cap.
    // Header for a varint-variant root is 2 bytes: tag + varuint name length.
    assert_eq!(to_varint_bytes(&Value::Int(i32::MIN)).unwrap().len(), 2 + 5);
    assert_eq!(
        to_varint_bytes(&Value::Long(i64::MIN)).unwrap().len(),
        2 + 10
    );
}

// --- string length --------------------------------------------------------

/// The varint variant's string length is unbounded by its prefix, so it is
/// checked explicitly — and *before* any allocation, so a 3-byte length cannot
/// make the decoder commit to a large read.
#[test]
fn varint_string_length_over_max_is_rejected_before_allocating() {
    // varuint32 32768 = 0x80 0x80 0x02, with no payload following.
    let bytes = hex(concat!("0a00", "0801", "61", "808002"));
    assert!(
        from_varint_bytes::<Value>(&mut bytes.as_slice()).is_err(),
        "must reject an over-long string length"
    );
    // Stronger claim: it must be a *length-limit* rejection, not merely EOF, so
    // supplying the whole 32768-byte payload must not change the outcome.
    let full = {
        let mut v = hex(concat!("0a00", "0801", "61", "808002"));
        v.extend(std::iter::repeat_n(b'x', 32768));
        v.push(0x00);
        v
    };
    assert!(
        from_varint_bytes::<Value>(&mut full.as_slice()).is_err(),
        "a fully-present 32768-byte string must still be rejected"
    );
}

/// One byte over the limit, big-endian.
#[test]
fn string_one_byte_over_max_len_is_rejected_on_write() {
    let doc = compound([("s", Value::String(BString::from(vec![b'a'; 32768])))]);
    assert!(
        to_be_bytes(&doc).is_err(),
        "a 32768-byte string exceeds MAX_STRING_LEN and must be refused"
    );
}

/// Far over the limit, big-endian.
#[test]
fn string_far_over_max_len_is_rejected_on_write() {
    let value = compound([("s", Value::String(BString::from(vec![b'a'; 40_000])))]);
    assert!(
        to_be_bytes(&value).is_err(),
        "a 40000-byte string exceeds MAX_STRING_LEN and must be refused"
    );
}

/// The limit applies to every variant, not just the `u16`-prefixed ones.
#[test]
fn string_over_max_len_is_rejected_little_endian() {
    let v = compound([("a", Value::String(vec![b'x'; 40000].into()))]);
    assert!(to_le_bytes(&v).is_err());
}

/// The failure mode the guard prevents: `len as u16` on a 65536-byte string
/// wraps to 0, and the decoder then resynchronises on the string body as if it
/// were tag data. Encoding must therefore either fail or round-trip exactly —
/// never produce bytes that decode to something else.
#[test]
fn string_length_at_the_u16_wrap_point_does_not_corrupt() {
    let s = vec![b'x'; 65536];
    let v = compound([("a", Value::String(s.into()))]);
    match to_le_bytes(&v) {
        Err(_) => {} // desired: rejected up front
        Ok(bytes) => {
            let back: Result<Value, _> = from_le_bytes(&mut bytes.as_slice());
            let ok = matches!(&back, Ok(rt) if rt == &v);
            assert!(
                ok,
                "encoding a 65536-byte string must either error or round-trip"
            );
        }
    }
}

/// The same contract at 70000 bytes, where the `u16` prefix would wrap to 4464
/// and the trailing 65536 payload bytes would be re-read as tags.
#[test]
fn oversized_string_never_silently_corrupts_the_stream() {
    let original = BString::from(vec![b'a'; 70_000]);
    let doc = compound([("s", Value::String(original.clone()))]);
    let Ok(bytes) = to_be_bytes(&doc) else {
        return; // acceptable: rejected on write
    };
    let back: Value =
        from_be_bytes(&mut bytes.as_slice()).expect("encoded output must be decodable");
    assert_eq!(
        get(&back, "s"),
        &Value::String(original),
        "70000-byte string must round-trip, not corrupt the stream"
    );
}

/// The rejection is the specific `Error::StringTooLong`, and everything at or
/// below the limit still encodes — the guard must not be off by one.
#[test]
fn oversized_string_is_refused_and_max_len_still_encodes() {
    let value = compound([("s", Value::String(BString::from(vec![b'a'; 70_000])))]);
    let err = to_be_bytes(&value).expect_err("a 70000-byte string must be refused");
    assert!(matches!(err, nbtx::Error::StringTooLong(_)), "got {err:?}");

    let ok = BString::from(vec![b'a'; nbtx::MAX_STRING_LEN]);
    let value = compound([("s", Value::String(ok.clone()))]);
    let bytes = to_be_bytes(&value).unwrap();
    let back: Value = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(get(&back, "s"), &Value::String(ok));
}

/// Compound *keys* are NBT strings too, and are length-prefixed the same way, so
/// they go through the same check. One byte over the limit.
#[test]
fn compound_key_one_byte_over_max_len_is_rejected() {
    let key = BString::from(vec![b'k'; 32768]);
    let doc = Value::Compound(Compound::from([(key, Value::Int(1))]));
    assert!(
        to_be_bytes(&doc).is_err(),
        "a 32768-byte compound key exceeds MAX_STRING_LEN and must be refused"
    );
}

/// The same, far over the limit — where an unchecked `u16` cast would wrap and
/// corrupt the stream rather than merely truncate the key.
#[test]
fn compound_key_far_over_max_len_is_rejected() {
    let value = Value::Compound(Compound::from([(
        BString::from("a".repeat(40_000)),
        Value::Int(1),
    )]));
    assert!(to_be_bytes(&value).is_err(), "a 40000-byte key must error");
}

// --- nesting depth --------------------------------------------------------

/// 2000 nested compounds, little-endian. Runs on an **ordinary** libtest thread
/// deliberately: the `Value` path is the one that matters for untrusted input,
/// so it must fit the 2 MiB std default even in an unoptimised build. Measured,
/// it peaks at ~1.15 MB at the 512-container bound. Only the derived-struct
/// reflection path needs an oversized thread; see
/// [`deep_nesting_is_rejected_for_derived_structs`].
#[test]
fn deep_nesting_is_rejected_on_decode() {
    let mut bytes = hex("0a0000");
    for _ in 0..2000 {
        bytes.extend(hex("0a010061"));
    }
    bytes.extend(std::iter::repeat_n(0x00u8, 2001));

    let err = from_le_bytes::<Value>(&mut bytes.as_slice())
        .expect_err("2000 nested compounds must be refused");
    assert!(
        matches!(err, nbtx::Error::MaxDepthExceeded(_)),
        "expected MaxDepthExceeded, got {err:?}"
    );
}

/// 700 nested compounds, big-endian, built by hand so the test does not depend
/// on the encoder's own recursion to produce its input.
#[test]
fn deep_compound_nesting_is_rejected() {
    let depth = 700usize;
    let mut buf = vec![10u8]; // root compound
    be::str_payload(&mut buf, b"");
    for _ in 0..depth {
        buf.push(10); // TAG_Compound
        be::str_payload(&mut buf, b"n");
    }
    buf.push(1); // innermost TAG_Byte
    be::str_payload(&mut buf, b"leaf");
    buf.push(7);
    for _ in 0..=depth {
        be::end(&mut buf);
    }
    let err = from_be_bytes::<Value>(&mut buf.as_slice())
        .expect_err("a depth-bounded reader must reject 700-deep nesting");
    assert!(
        matches!(err, nbtx::Error::MaxDepthExceeded(_)),
        "got {err:?}"
    );
}

/// Nested lists are the cheapest deep document an attacker can send: 50 000
/// levels here cost 5 bytes each, so ~250 KB of input used to abort the process.
#[test]
fn deep_list_nesting_is_rejected_rather_than_overflowing_the_stack() {
    // Root compound -> "l" -> 50000 nested single-element TAG_Lists.
    let mut buf = be::root_compound();
    be::entry_header(&mut buf, 9, b"l");
    for _ in 0..50_000 {
        buf.push(9); // element type = TAG_List
        buf.extend_from_slice(&1i32.to_be_bytes()); // one element
    }
    buf.push(0); // innermost: element type TAG_End
    buf.extend_from_slice(&0i32.to_be_bytes()); // length 0
    be::end(&mut buf);

    let err = from_be_bytes::<Value>(&mut buf.as_slice())
        .expect_err("50000 nested lists must be refused");
    assert!(
        matches!(err, nbtx::Error::MaxDepthExceeded(_)),
        "expected MaxDepthExceeded, got {err:?}"
    );
}

/// The bound is exactly `MAX_DEPTH` containers: a document at the limit still
/// decodes, one level deeper does not. Guards against the limit silently
/// drifting, in either direction.
#[test]
fn depth_limit_boundary_is_exactly_max_depth() {
    fn nested_lists(depth: usize) -> Vec<u8> {
        // Root compound (container 1) -> "l" -> (depth - 1) nested lists.
        let mut buf = be::root_compound();
        be::entry_header(&mut buf, 9, b"l");
        for _ in 0..depth - 2 {
            buf.push(9);
            buf.extend_from_slice(&1i32.to_be_bytes());
        }
        buf.push(0);
        buf.extend_from_slice(&0i32.to_be_bytes());
        be::end(&mut buf);
        buf
    }

    assert!(
        from_be_bytes::<Value>(&mut nested_lists(nbtx::MAX_DEPTH).as_slice()).is_ok(),
        "a document exactly at MAX_DEPTH must still decode"
    );
    assert!(
        from_be_bytes::<Value>(&mut nested_lists(nbtx::MAX_DEPTH + 1).as_slice()).is_err(),
        "one container past MAX_DEPTH must be rejected"
    );
}

// --- speculative preallocation -------------------------------------------

/// A chain of nested `TAG_List`s, each declaring `i32::MAX` elements, in the
/// smallest possible input: 5 bytes per level (element-type byte + length
/// prefix) under a root `TAG_List` with an empty name, closed by an empty
/// `TAG_End` list.
///
/// `levels` list headers, so the innermost sits `levels - 1` containers deep —
/// inside `MAX_DEPTH` at 511, which is the point: the document is rejected for
/// running out of *bytes*, and every one of the live frames has already made
/// its up-front reservation by then. `tail` is whatever follows the last
/// header.
fn nested_huge_lists(levels: usize, tail: &[u8]) -> Vec<u8> {
    let mut buf = vec![9u8]; // root TAG_List
    be::str_payload(&mut buf, b""); // empty name
    for _ in 0..levels {
        buf.push(9); // element type = TAG_List
        buf.extend_from_slice(&i32::MAX.to_be_bytes());
    }
    buf.extend_from_slice(tail);
    buf
}

/// 2.5 KB of input must not let the decoder reserve gigabytes.
///
/// Each of the up-to-`MAX_DEPTH` live `read_list` frames reserves capacity for
/// its declared length *before* any element is read, so a per-element cap would
/// be multiplied by the nesting depth (4096 × `size_of::<ValueList>()` × 511 ≈
/// 64 MiB from this input). The cap is in bytes instead, which bounds the whole
/// chain at `MAX_DEPTH × 4 KiB`.
///
/// The observable contract this test can pin portably is the one that must hold
/// either way: the document is refused, with an error, and without a panic or
/// an allocation failure.
#[test]
fn deeply_nested_huge_list_lengths_are_refused_without_panicking() {
    // 511 headers, then an empty `TAG_End` list: 2563 bytes in total.
    let bytes = nested_huge_lists(511, &[0, 0, 0, 0, 0]);
    assert_eq!(bytes.len(), 2563, "the attack document stays tiny");

    let err = from_be_bytes::<Value>(&mut bytes.as_slice())
        .expect_err("511 nested lists each promising i32::MAX elements must be refused");
    assert!(
        matches!(
            err,
            nbtx::Error::UnexpectedEof(_) | nbtx::Error::MaxDepthExceeded(_)
        ),
        "expected UnexpectedEof or MaxDepthExceeded, got {err:?}"
    );
}

/// The same chain ending in a list of `Compound`s — the larger element type, and
/// so the worse case for a count-based cap.
#[test]
fn deeply_nested_huge_list_of_compounds_is_refused_without_panicking() {
    // 510 list headers, then one promising `i32::MAX` compounds that never come.
    let mut bytes = nested_huge_lists(510, &[]);
    bytes.push(10); // element type = TAG_Compound
    bytes.extend_from_slice(&i32::MAX.to_be_bytes());

    let err = from_be_bytes::<Value>(&mut bytes.as_slice())
        .expect_err("a nested list-of-compound chain promising i32::MAX elements must be refused");
    assert!(
        matches!(
            err,
            nbtx::Error::UnexpectedEof(_) | nbtx::Error::MaxDepthExceeded(_)
        ),
        "expected UnexpectedEof or MaxDepthExceeded, got {err:?}"
    );
}

/// The guard must apply to the **encoder** too, otherwise building a deep
/// `Value` in memory and serialising it would still overflow the stack.
#[test]
fn deep_nesting_is_rejected_on_encode() {
    let mut v = Value::Int(1);
    for _ in 0..2000 {
        v = Value::List(nbtx::ValueList::try_from(vec![v]).expect("a singleton"));
    }
    let err = to_le_bytes(&v).expect_err("encoding a 2000-deep list must be refused");
    assert!(
        matches!(err, nbtx::Error::MaxDepthExceeded(_)),
        "expected MaxDepthExceeded, got {err:?}"
    );
}

/// A *derived* recursive struct must hit the same bound — the guard lives in the
/// facet-reflection read path (`read_seq`/`read_struct`), not only in the
/// `Value` fast path.
///
/// Runs on an explicitly-sized thread. The reflection path costs about 1 KB of
/// stack per container in an optimised build (so `MAX_DEPTH` × 1 KB is
/// comfortably within any default stack), but roughly 6 KB in an unoptimised
/// one, where LLVM gives every temporary in a function its own never-reused
/// slot. `cargo test` builds unoptimised, and libtest's threads get the 2 MiB
/// std default, which the derived-struct path alone would exhaust before
/// reaching 512. The `Value` path — the one that matters for untrusted input,
/// since decoding unknown data into a derived recursive type is a much narrower
/// scenario — is an order of magnitude cheaper and is covered above on a plain
/// test thread.
#[test]
fn deep_nesting_is_rejected_for_derived_structs() {
    #[derive(facet::Facet, Debug)]
    struct Nest {
        #[facet(rename = "a")]
        a: Vec<Nest>,
    }

    // Each level is `{ "a": List<Compound>[ <next level> ] }`, i.e. two nested
    // containers (the compound and the list) per level.
    let mut bytes = hex("0a0000");
    for _ in 0..600 {
        bytes.extend(hex("09010061" /* List "a" */));
        bytes.extend(hex("0a" /* elem type Compound */));
        bytes.extend(hex("01000000" /* one element */));
    }
    bytes.extend(hex("09010061000000000000")); // innermost: empty list, then End
    bytes.extend(std::iter::repeat_n(0x00u8, 600)); // close each compound

    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            let err = from_le_bytes::<Nest>(&mut bytes.as_slice())
                .expect_err("a deeply nested derived struct must error, not overflow the stack");
            assert!(
                matches!(err, nbtx::Error::MaxDepthExceeded(_)),
                "expected MaxDepthExceeded, got {err:?}"
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
