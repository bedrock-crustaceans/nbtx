//! The public entry points themselves: the `_in` writer functions, the
//! [`nbtx::Serializer`] types, the generic `to_bytes`/`from_bytes`, and
//! [`nbtx::Named`]'s inherent methods.
//!
//! Everywhere else the suite reaches for `to_be_bytes`/`from_be_bytes` because
//! they are convenient. The other entry points are what a caller uses when they
//! already have a socket, a file handle, or an endianness chosen at runtime —
//! and none of them is exercised by a test, only by the examples.
//!
//! The reader-position behaviour below is the part with no other coverage at
//! all: `from_*_bytes` takes `&mut impl Read`, so where it leaves that reader
//! decides whether a caller can decode a second document from the same stream.

#![cfg(feature = "nbt")]

use bstr::BString;
use facet::Facet;
use nbtx::{
    BigEndian, Compound, EndiannessImpl, LittleEndian, Named, Serializer, Value, ValueList,
    Variant, VarintEndian, from_be_bytes, from_bytes, to_be_bytes, to_be_bytes_in, to_bytes,
    to_bytes_in, to_le_bytes_in, to_varint_bytes, to_varint_bytes_in,
};

fn comp(entries: &[(&str, Value)]) -> Value {
    Value::Compound(
        entries
            .iter()
            .map(|(k, v)| (BString::from(*k), v.clone()))
            .collect::<Compound>(),
    )
}

/// The `_in` variants must produce exactly what the allocating ones do — they
/// are the same encoder, so any difference is a plumbing bug.
#[test]
fn the_in_writer_variants_match_their_allocating_counterparts() {
    let doc = comp(&[("a", Value::Int(1)), ("s", Value::String("hi".into()))]);

    let mut be = Vec::new();
    to_be_bytes_in(&mut be, &doc).unwrap();
    assert_eq!(be, to_be_bytes(&doc).unwrap());

    let mut le = Vec::new();
    to_le_bytes_in(&mut le, &doc).unwrap();
    assert_eq!(le, nbtx::to_le_bytes(&doc).unwrap());

    let mut var = Vec::new();
    to_varint_bytes_in(&mut var, &doc).unwrap();
    assert_eq!(var, to_varint_bytes(&doc).unwrap());
}

/// An `_in` writer **appends**: it does not clear or seek the writer first. That
/// is what lets a caller frame several documents into one buffer, and it is the
/// behaviour a caller would be surprised by if it were the other way round.
#[test]
fn an_in_writer_appends_to_an_existing_buffer() {
    let mut buf = b"HEADER".to_vec();
    to_be_bytes_in(&mut buf, &Value::Int(1)).unwrap();
    to_be_bytes_in(&mut buf, &Value::Int(2)).unwrap();

    assert!(buf.starts_with(b"HEADER"));
    let mut rest = &buf[6..];
    assert_eq!(from_be_bytes::<Value>(&mut rest).unwrap(), Value::Int(1));
    assert_eq!(from_be_bytes::<Value>(&mut rest).unwrap(), Value::Int(2));
    assert!(rest.is_empty(), "both documents must have been consumed");
}

/// `from_*_bytes` stops immediately after the document it read, so consecutive
/// documents in one stream can be decoded one after another. Nothing else in the
/// suite decodes twice from a single reader, yet this is exactly how a Bedrock
/// network stream is consumed.
#[test]
fn a_reader_is_left_positioned_after_the_document_it_decoded() {
    let mut stream = Vec::new();
    to_be_bytes_in(&mut stream, &comp(&[("first", Value::Byte(1))])).unwrap();
    to_be_bytes_in(&mut stream, &Value::String("second".into())).unwrap();
    to_be_bytes_in(&mut stream, &Value::List(ValueList::Long(vec![3]))).unwrap();
    stream.extend_from_slice(b"trailing bytes that are not NBT");

    let mut cursor = stream.as_slice();
    assert_eq!(
        from_be_bytes::<Value>(&mut cursor).unwrap(),
        comp(&[("first", Value::Byte(1))])
    );
    assert_eq!(
        from_be_bytes::<Value>(&mut cursor).unwrap(),
        Value::String("second".into())
    );
    assert_eq!(
        from_be_bytes::<Value>(&mut cursor).unwrap(),
        Value::List(ValueList::Long(vec![3]))
    );
    assert_eq!(
        cursor, b"trailing bytes that are not NBT",
        "trailing data must be left untouched for the caller"
    );
}

/// The same for the varint variant, whose length prefixes make document
/// boundaries depend on the values themselves rather than on fixed widths.
#[test]
fn consecutive_varint_documents_decode_from_one_reader() {
    let mut stream = Vec::new();
    for n in [0i32, 1, -1, i32::MAX, i32::MIN] {
        to_varint_bytes_in(&mut stream, &Value::Int(n)).unwrap();
    }
    let mut cursor = stream.as_slice();
    for n in [0i32, 1, -1, i32::MAX, i32::MIN] {
        assert_eq!(
            nbtx::from_varint_bytes::<Value>(&mut cursor).unwrap(),
            Value::Int(n)
        );
    }
    assert!(cursor.is_empty());
}

/// A failing `_in` write is **not** atomic: the encoder streams, so whatever it
/// managed to emit before the error is already in the writer. Callers must not
/// reuse the buffer after a failure, and this pins that so the expectation is
/// explicit rather than assumed either way.
#[test]
fn a_failed_in_write_leaves_a_partial_document_behind() {
    // The over-long string is detected only once the encoder has already
    // written the root header and the entry's tag and key.
    let doc = comp(&[
        ("ok", Value::Int(1)),
        (
            "bad",
            Value::String(bstr::BString::from(vec![b'x'; nbtx::MAX_STRING_LEN + 1])),
        ),
    ]);
    let mut buf = Vec::new();
    let err = to_be_bytes_in(&mut buf, &doc).expect_err("must fail");
    assert!(matches!(err, nbtx::Error::StringTooLong(_)));
    assert!(
        !buf.is_empty(),
        "the encoder streams, so a failure leaves partial output"
    );
    assert!(
        from_be_bytes::<Value>(&mut buf.as_slice()).is_err(),
        "and that partial output is not a valid document"
    );
}

/// The generic `to_bytes::<E>` / `from_bytes::<E, T>` take the endianness as a
/// type parameter, which is what lets a caller be generic over the variant
/// instead of matching on it. Each marker must agree with its named shortcut.
#[test]
fn the_generic_entry_points_agree_with_the_named_shortcuts() {
    let doc = comp(&[("a", Value::Int(300))]);

    assert_eq!(
        to_bytes::<BigEndian>(&doc).unwrap(),
        to_be_bytes(&doc).unwrap()
    );
    assert_eq!(
        to_bytes::<LittleEndian>(&doc).unwrap(),
        nbtx::to_le_bytes(&doc).unwrap()
    );
    assert_eq!(
        to_bytes::<VarintEndian>(&doc).unwrap(),
        to_varint_bytes(&doc).unwrap()
    );

    let bytes = to_bytes::<LittleEndian>(&doc).unwrap();
    assert_eq!(
        from_bytes::<LittleEndian, Value>(&mut bytes.as_slice()).unwrap(),
        doc
    );
    // ...and `to_bytes_in` likewise.
    let mut buf = Vec::new();
    to_bytes_in::<VarintEndian>(&mut buf, &doc).unwrap();
    assert_eq!(buf, to_varint_bytes(&doc).unwrap());
}

/// A function generic over `EndiannessImpl` must actually compile and work for
/// all three markers — the point of the sealed trait. This is the shape a
/// caller writes when the variant is a configuration value.
#[test]
fn a_caller_can_be_generic_over_the_endianness_marker() {
    fn roundtrip<E: EndiannessImpl>(v: &Value) -> Value {
        let bytes = to_bytes::<E>(v).unwrap();
        from_bytes::<E, Value>(&mut bytes.as_slice()).unwrap()
    }
    let doc = comp(&[("n", Value::Long(-1))]);
    assert_eq!(roundtrip::<BigEndian>(&doc), doc);
    assert_eq!(roundtrip::<LittleEndian>(&doc), doc);
    assert_eq!(roundtrip::<VarintEndian>(&doc), doc);
}

/// `Variant` is the runtime mirror of the type-level markers, and the mapping
/// between them must be exact — the codecs branch on it for every primitive.
#[test]
fn each_endianness_marker_maps_to_its_runtime_variant() {
    assert_eq!(<BigEndian as EndiannessImpl>::AS_ENUM, Variant::BigEndian);
    assert_eq!(
        <LittleEndian as EndiannessImpl>::AS_ENUM,
        Variant::LittleEndian
    );
    assert_eq!(
        <VarintEndian as EndiannessImpl>::AS_ENUM,
        Variant::VarintEndian
    );
    // The three are distinct, so a mis-mapped marker cannot go unnoticed.
    assert_ne!(Variant::BigEndian, Variant::LittleEndian);
    assert_ne!(Variant::LittleEndian, Variant::VarintEndian);
}

/// The `Serializer` struct is the reusable form of the writer: it holds the
/// writer between calls and hands it back at the end. Serialising twice into one
/// serializer must produce the same stream as two `_in` calls.
#[test]
fn the_serializer_struct_reuses_its_writer_and_returns_it() {
    let mut ser: Serializer<Vec<u8>, BigEndian> = Serializer::new(Vec::new());
    ser.serialize(&Value::Int(1)).unwrap();
    ser.serialize(&Value::Int(2)).unwrap();
    let out = ser.into_inner();

    let mut expected = Vec::new();
    to_be_bytes_in(&mut expected, &Value::Int(1)).unwrap();
    to_be_bytes_in(&mut expected, &Value::Int(2)).unwrap();
    assert_eq!(out, expected);
}

/// The `Serializer` is generic over its endianness too, and over any
/// `io::Write` — including one that is not a `Vec`.
#[test]
fn the_serializer_struct_works_for_every_variant_and_writer() {
    let mut buf = [0u8; 32];
    {
        let mut ser: Serializer<&mut [u8], VarintEndian> = Serializer::new(&mut buf[..]);
        ser.serialize(&Value::Int(7)).unwrap();
    }
    // varint: tag 3, empty name, zigzag(7) = 14.
    assert_eq!(&buf[..3], &[0x03, 0x00, 0x0e]);

    let mut ser: Serializer<Vec<u8>, LittleEndian> = Serializer::new(Vec::new());
    ser.serialize(&Value::Short(300)).unwrap();
    assert_eq!(
        ser.into_inner(),
        nbtx::to_le_bytes(&Value::Short(300)).unwrap()
    );
}

/// The entry points accept unsized values (`&str`, `&[T]`, `&Value`), because
/// their bound is `impl Facet + ?Sized`. Dropping the `?Sized` would be a
/// silent, source-breaking narrowing.
#[test]
fn the_entry_points_accept_unsized_values() {
    let bytes = to_be_bytes("hello").unwrap();
    assert_eq!(
        from_be_bytes::<String>(&mut bytes.as_slice()).unwrap(),
        "hello"
    );

    let slice: &[i32] = &[1, 2, 3];
    let bytes = to_be_bytes(slice).unwrap();
    assert_eq!(
        from_be_bytes::<Value>(&mut bytes.as_slice()).unwrap(),
        Value::IntArray(vec![1, 2, 3])
    );
}

/// Any `io::Read` works as a source, not just a slice — a caller decoding from a
/// file or socket goes through the same generic parameter.
#[test]
fn decoding_works_from_any_io_read() {
    let bytes = to_be_bytes(&comp(&[("a", Value::Int(1))])).unwrap();
    let mut cursor = std::io::Cursor::new(bytes.clone());
    assert_eq!(
        from_be_bytes::<Value>(&mut cursor).unwrap(),
        comp(&[("a", Value::Int(1))])
    );

    // A reader that yields one byte at a time must work too: nothing in the
    // decoder may assume a read returns everything it asked for.
    struct Trickle<'a>(&'a [u8]);
    impl std::io::Read for Trickle<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.0.is_empty() || buf.is_empty() {
                return Ok(0);
            }
            buf[0] = self.0[0];
            self.0 = &self.0[1..];
            Ok(1)
        }
    }
    let mut trickle = Trickle(&bytes);
    assert_eq!(
        from_be_bytes::<Value>(&mut trickle).unwrap(),
        comp(&[("a", Value::Int(1))])
    );
}

/// A larger document through the byte-at-a-time reader, so the bounded array and
/// string readers are exercised across the same short-read boundary — those use
/// `read_to_end` rather than `read_exact`, which is the part most likely to
/// mishandle a partial read.
#[test]
fn a_trickling_reader_still_fills_arrays_and_strings() {
    struct Trickle<'a>(&'a [u8]);
    impl std::io::Read for Trickle<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.0.len().min(buf.len()).min(3);
            buf[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            Ok(n)
        }
    }
    let doc = comp(&[
        (
            "bytes",
            Value::ByteArray((0..500).map(|i| i as u8).collect()),
        ),
        ("text", Value::String(BString::from("x".repeat(500)))),
        ("ints", Value::IntArray((0..200).collect())),
    ]);
    let bytes = to_be_bytes(&doc).unwrap();
    let mut trickle = Trickle(&bytes);
    assert_eq!(from_be_bytes::<Value>(&mut trickle).unwrap(), doc);
}

/// `Named::new` and `Named::into_inner` are the constructor and accessor pair
/// that let the wrapper be added and removed without naming its fields.
#[test]
fn named_new_and_into_inner() {
    let named = Named::new("root", Value::Int(7));
    assert_eq!(named.name, "root");
    assert_eq!(named.value, Value::Int(7));
    assert_eq!(named.into_inner(), Value::Int(7));

    // `new` takes anything that converts into a `BString`, including raw bytes,
    // because a root name is an ordinary NBT string.
    let raw = Named::new(BString::from(vec![0xffu8, 0xfe]), 1i32);
    let bytes = to_be_bytes(&raw).unwrap();
    let back: Named<i32> = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(back, raw);
}

/// A `Named` is only special at the *root*: as a struct field it is an ordinary
/// two-key compound, which is what the doc comment promises.
#[test]
fn a_named_field_is_an_ordinary_compound() {
    #[derive(Facet, Debug, PartialEq)]
    struct Holder {
        inner: Named<i32>,
    }
    let h = Holder {
        inner: Named::new("not a root name", 5),
    };
    let bytes = to_be_bytes(&h).unwrap();
    let as_value: Value = from_be_bytes(&mut bytes.as_slice()).unwrap();
    let inner = as_value.as_compound().unwrap()[&BString::from("inner")]
        .as_compound()
        .unwrap();
    assert_eq!(inner.len(), 2, "name and value are ordinary keys here");
    assert_eq!(
        inner.get(&BString::from("name")),
        Some(&Value::String("not a root name".into()))
    );

    let back: Holder = from_be_bytes(&mut bytes.as_slice()).unwrap();
    assert_eq!(back, h);
}

/// The public limits are constants a caller can branch on before encoding, so
/// they must hold the documented values and stay consistent with what the codec
/// actually enforces.
#[test]
fn the_public_limit_constants_match_what_is_enforced() {
    assert_eq!(nbtx::MAX_DEPTH, 512);
    assert_eq!(nbtx::MAX_STRING_LEN, i16::MAX as usize);

    // A string of exactly the constant encodes; one byte more does not.
    let at = Value::String(BString::from(vec![b'a'; nbtx::MAX_STRING_LEN]));
    let over = Value::String(BString::from(vec![b'a'; nbtx::MAX_STRING_LEN + 1]));
    assert!(to_be_bytes(&at).is_ok());
    assert!(to_be_bytes(&over).is_err());
}

/// The SNBT codec's own `Serializer`/`Deserializer` structs, which are exported
/// alongside the `to_string`/`from_string` shortcuts and are the way to
/// serialise several values into one buffer.
#[cfg(feature = "snbt")]
#[test]
fn the_snbt_serializer_and_deserializer_structs_work() {
    let mut ser = nbtx::snbt::Serializer::new();
    ser.serialize(&Value::Int(1)).unwrap();
    ser.serialize(&Value::Byte(2)).unwrap();
    assert_eq!(ser.into_inner(), "12b", "the serializer appends");

    // The default-constructed form must behave identically to `new`.
    let mut ser = nbtx::snbt::Serializer::default();
    ser.serialize(&comp(&[("a", Value::Int(1))])).unwrap();
    assert_eq!(ser.into_inner(), "{a:1}");

    let mut de = nbtx::snbt::Deserializer::new("{a:1,b:[I;2]}");
    let v: Value = de.parse().unwrap();
    assert_eq!(
        v,
        comp(&[("a", Value::Int(1)), ("b", Value::IntArray(vec![2]))])
    );

    // `to_string` is the shortcut for the serializer, and must agree with it.
    let doc = comp(&[("k", Value::Long(9))]);
    let mut ser = nbtx::snbt::Serializer::new();
    ser.serialize(&doc).unwrap();
    assert_eq!(ser.into_inner(), nbtx::to_string(&doc).unwrap());
}
