//! Round-trips of real NBT files: decode one to a `Value`, re-encode it, and
//! compare byte-for-byte. With the default `preserve_order` feature compound key
//! order is preserved, so re-encoding a big-endian fixture reproduces it exactly.
//!
//! The files themselves live in `tests/fixtures/`, alongside `examples/servers.dat`.

#![cfg(feature = "nbt")]

use facet::Facet;
use nbtx::{Named, Value, from_be_bytes, to_be_bytes};

const BIG_TEST_NBT: &[u8] = include_bytes!("fixtures/bigtest.nbt");
const HELLO_WORLD_NBT: &[u8] = include_bytes!("fixtures/hello_world.nbt");
#[cfg(feature = "preserve_order")]
const PLAYER_NAN_VALUE_NBT: &[u8] = include_bytes!("fixtures/player_nan_value.nbt");
#[cfg(feature = "preserve_order")]
const SERVERS_DAT: &[u8] = include_bytes!("../examples/servers.dat");

// Byte-identity depends on compound key order being preserved, which only holds
// under the `preserve_order` feature (a sorted `BTreeMap` would re-order keys).
#[cfg(feature = "preserve_order")]
#[test]
fn fixtures_reencode_byte_identically() {
    // With `preserve_order`, a `Value` round-trip of these two is
    // byte-identical to the original. This is the strongest fixture assertion
    // available through `Value`: both have an empty root name and compounds
    // carrying several keys, so byte identity proves on-disk key order is
    // reproduced exactly (a `BTreeMap` would have re-sorted them).
    //
    // `player_nan_value.nbt` additionally holds an empty `List` whose on-disk
    // element type is `Byte`. That used to be the one thing a bare `Value`
    // could not carry — an empty `Vec<Value>` had nowhere to record it — and it
    // re-encoded as the canonical `End`. `Value::List` now holds a `ValueList`,
    // which names an element type whether or not it has elements, so the
    // fixture is byte-identical too.
    for (label, fixture) in [
        ("servers.dat", SERVERS_DAT),
        ("player_nan_value.nbt", PLAYER_NAN_VALUE_NBT),
    ] {
        let value: Value = from_be_bytes(&mut fixture.to_vec().as_slice()).unwrap();
        let encoded = to_be_bytes(&value).unwrap();
        assert_eq!(
            encoded.as_slice(),
            fixture,
            "{label} did not re-encode byte-identically"
        );
    }
}

#[test]
fn fixtures_reencode_stably() {
    // These two cannot be *byte-identical* through a bare `Value` for a reason
    // orthogonal to key order: `hello world` / `Level` carry a non-empty root
    // compound name, which a bare `Value` discards on read and re-emits empty
    // (decode into `Named<Value>` to keep it — see
    // `hello_world_named_root_is_byte_identical`). The re-encode is nonetheless
    // a fixpoint (byte-stable), which confirms the codec itself is lossless up
    // to that one normalisation.
    for fixture in [HELLO_WORLD_NBT, BIG_TEST_NBT] {
        let v1: Value = from_be_bytes(&mut fixture.to_vec().as_slice()).unwrap();
        let e1 = to_be_bytes(&v1).unwrap();
        let v2: Value = from_be_bytes(&mut e1.as_slice()).unwrap();
        let e2 = to_be_bytes(&v2).unwrap();
        assert_eq!(e1, e2, "re-encode was not byte-stable");
    }
}

#[test]
fn read_write_hello_world_byte_identical() {
    #[derive(Facet, Debug, PartialEq)]
    struct HelloWorld {
        name: Value,
    }

    // `hello_world.nbt`'s root name is `hello world`. The plain entry points
    // always write an *empty* root name (see `Named`), so byte identity for a
    // named-root document requires the explicit `Named<T>` wrapper.
    let decoded: Named<HelloWorld> =
        from_be_bytes(&mut HELLO_WORLD_NBT.to_vec().as_slice()).unwrap();
    assert_eq!(decoded.name, "hello world");
    let encoded = to_be_bytes(&decoded).unwrap();
    assert_eq!(encoded.as_slice(), HELLO_WORLD_NBT);
}

/// The same fixture through the dynamic `Value` path: `Named<Value>` keeps the
/// root name, so the document round-trips byte-identically without a schema.
#[test]
fn hello_world_named_root_is_byte_identical() {
    let decoded: Named<Value> = from_be_bytes(&mut HELLO_WORLD_NBT.to_vec().as_slice()).unwrap();
    assert_eq!(decoded.name, "hello world");
    assert_eq!(
        to_be_bytes(&decoded).unwrap().as_slice(),
        HELLO_WORLD_NBT,
        "Named<Value> must reproduce the root name byte-for-byte"
    );
}
