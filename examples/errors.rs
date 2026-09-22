//! What nbtx does with hostile, truncated and merely wrong input.
//!
//! `cargo run --example errors`
//!
//! Every entry point returns `Result<_, nbtx::Error>`; none of them panic, and
//! none of them abort. That is a deliberate property rather than an accident:
//! NBT arrives from network packets and from world files an attacker may
//! control, so a length prefix claiming four gigabytes, a list nested ten
//! thousand deep, or a varint that never terminates all have to come back as
//! ordinary errors. This example triggers each one on purpose.
//!
//! `nbtx::Error` is a plain enum, so you can match on the variant and read
//! typed detail off it — the messages below are all built from accessors, not
//! from string parsing.

use bstr::BString;
use byteorder::BigEndian;
use facet::Facet;
use nbtx::{Error, Value, VarintEndian};

#[derive(Facet, Debug)]
struct Message {
    text: String,
}

#[derive(Facet, Debug)]
struct Count {
    text: i32,
}

fn main() {
    let good = nbtx::to_be_bytes(&Message {
        text: "hello".to_owned(),
    })
    .expect("a valid document");

    report("a tag byte outside the valid range 0-12", || {
        // 0x63 is not a tag. The first byte of any document is its root tag, so
        // this is the cheapest possible garbage input.
        nbtx::from_be_bytes::<Value>(&mut [0x63, 0x00, 0x00].as_slice()).map(drop)
    });

    report("the stream ends mid-value", || {
        // Truncation is what a dropped connection or a half-written file looks
        // like. Note that the *declared* length is never trusted to size an
        // allocation, so a bogus multi-gigabyte length cannot exhaust memory
        // before the read fails.
        nbtx::from_be_bytes::<Value>(&mut &good[..good.len() / 2]).map(drop)
    });

    report(
        "a field holds a different tag than the struct expects",
        || {
            // `Message::text` was written as a String tag; `Count::text` wants an
            // Int, so the error carries both tags (see `detail` below).
            nbtx::from_be_bytes::<Count>(&mut good.as_slice()).map(drop)
        },
    );

    report(
        "a compound key that the target struct has no field for",
        || {
            #[derive(Facet, Debug)]
            struct Empty {}
            nbtx::from_be_bytes::<Empty>(&mut good.as_slice()).map(drop)
        },
    );

    report("containers nested past MAX_DEPTH", || {
        // Recursive descent plus unbounded nesting is a stack overflow, and a
        // stack overflow in Rust aborts the process — not something a decoder
        // of untrusted input may do. Both codecs count their depth instead and
        // stop at `nbtx::MAX_DEPTH`, on read *and* on write.
        nbtx::from_bytes::<BigEndian, Value>(&mut nested_lists(nbtx::MAX_DEPTH + 8).as_slice())
            .map(drop)
    });

    report("a string longer than MAX_STRING_LEN", || {
        // The big/little-endian variants prefix strings with a `u16`, so a
        // longer string cannot be represented. Truncating the length silently
        // (the old behaviour) produced a stream nothing could decode; rejecting
        // it turns silent corruption into an error at the point of the mistake.
        let oversized = Value::String(BString::from(vec![b'a'; nbtx::MAX_STRING_LEN + 1]));
        nbtx::to_be_bytes(&oversized).map(drop)
    });

    report("a varint that never terminates", || {
        // Only the varint variant is exposed to this: every byte with the high
        // bit set continues the number, so an unbounded reader would shift past
        // the width of its target type. nbtx caps each varint at its type's
        // maximum width (5 bytes for 32-bit, 10 for 64-bit).
        let overlong = [0x03, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff];
        nbtx::from_bytes::<VarintEndian, Value>(&mut overlong.as_slice()).map(drop)
    });

    report("a list whose elements do not share one tag", || {
        // A `List` stores a single element-type byte for the whole list, so
        // this cannot be encoded at all. Writing it anyway would desync the
        // stream and make later keys vanish on the next read.
        // `Value::List` holds a `ValueList`, so the mixture is caught where the
        // list is built rather than on the way out.
        nbtx::ValueList::try_from(vec![Value::Int(1), Value::String("two".into())]).map(drop)
    });

    report("a type the codec has no NBT representation for", || {
        // A *unit* enum variant round-trips fine: it is written in whichever
        // form `#[facet(nbtx::variant_as(...))]` declares, and read back by
        // matching that name or discriminant. A variant carrying data has no
        // such encoding — NBT has no tagged union — so it is refused rather than
        // flattened into something lossy. `Value` is the one exception: it is an
        // enum, but the codecs special-case it.
        #[derive(Facet, Debug)]
        #[facet(nbtx::variant_as(str))]
        #[repr(u8)]
        enum Mode {
            Custom(#[allow(dead_code)] i32),
        }
        #[derive(Facet, Debug)]
        struct WithEnum {
            mode: Mode,
        }
        nbtx::to_be_bytes(&WithEnum {
            mode: Mode::Custom(7),
        })
        .map(drop)
    });

    report("an enum that never declared its wire form", || {
        // `#[facet(nbtx::variant_as(...))]` is mandatory: the shape of an enum
        // on the wire is part of a document's schema, so nbtx refuses to guess
        // one rather than let it change under a Rust-side edit.
        #[derive(Facet, Debug)]
        #[repr(u8)]
        enum Undeclared {
            Survival,
        }
        nbtx::to_be_bytes(&Undeclared::Survival).map(drop)
    });

    report("a discriminant too large for the declared width", || {
        // `variant_as(u8)` is one byte, and 300 does not fit in one. Truncating
        // it would write a number that decodes as a *different* variant, so the
        // value is refused instead.
        #[derive(Facet, Debug)]
        #[facet(nbtx::variant_as(u8))]
        #[repr(u16)]
        enum Wide {
            Big = 300,
        }
        nbtx::to_be_bytes(&Wide::Big).map(drop)
    });

    println!("\nall of the above returned an error; none panicked or aborted.");
    println!(
        "limits in force: MAX_DEPTH = {}, MAX_STRING_LEN = {}",
        nbtx::MAX_DEPTH,
        nbtx::MAX_STRING_LEN
    );
}

/// Runs one deliberately-broken operation and prints the error it produced.
fn report(what: &str, op: impl Fn() -> Result<(), Error>) {
    println!("\n{what}:");
    match op() {
        Ok(()) => println!("    UNEXPECTEDLY SUCCEEDED"),
        Err(err) => {
            println!("    {} -> {err}", variant_name(&err));
            if let Some(detail) = detail(&err) {
                println!("    typed detail: {detail}");
            }
        }
    }
}

/// Pulls structured data back out of an error without touching its message.
///
/// Each variant is a struct with accessors, so a caller can react to *what*
/// went wrong (retry, repair, report a byte offset) instead of matching on
/// display strings. Build with the `error-context` feature for the additional
/// `at()`/`index()` accessors — though the codecs do not yet thread real
/// context through, so `at()` is `"unknown"` everywhere but the document root
/// and `index()` is always `None`.
fn detail(err: &Error) -> Option<String> {
    Some(match err {
        Error::UnexpectedType(e) => {
            format!("expected tag {}, found tag {}", e.expected(), e.found())
        }
        Error::UnknownField(e) => {
            format!("key {:?} on struct `{}`", e.field(), e.container())
        }
        Error::MaxDepthExceeded(e) => format!("limit was {} containers", e.max()),
        Error::StringTooLong(e) => format!("{} bytes, limit {}", e.len(), e.max()),
        Error::InvalidVarint(e) => format!("gave up after {} bytes", e.max_bytes()),
        Error::TypeOutOfRange(e) => format!("tag byte {:#04x}", e.found()),
        Error::HeterogeneousList { expected, found } => {
            format!("first element is {expected}, a later one is {found}")
        }
        Error::Unsupported(e) => e.operation().to_owned(),
        Error::MissingVariantAs(e) => format!("enum `{}`", e.container()),
        Error::DiscriminantOutOfRange(e) => format!(
            "`{}::{}` is {}, which does not fit `variant_as({})`",
            e.container(),
            e.variant(),
            e.discriminant(),
            e.mode()
        ),
        Error::InvalidLenientWidth(e) => format!(
            "`{}::{}` cannot be widened: {}",
            e.container(),
            e.field(),
            e.reason()
        ),
        Error::LenientWidthOutOfRange(e) => format!(
            "{} arrived as {}, which `{}` cannot hold exactly",
            e.value(),
            e.from(),
            e.target()
        ),
        _ => return None,
    })
}

/// The `Error` variant name, to show which one each input actually produced.
fn variant_name(err: &Error) -> &'static str {
    match err {
        Error::TypeOutOfRange(_) => "Error::TypeOutOfRange",
        Error::UnexpectedType(_) => "Error::UnexpectedType",
        Error::UnexpectedEnd(_) => "Error::UnexpectedEnd",
        Error::Unsupported(_) => "Error::Unsupported",
        Error::HeterogeneousList { .. } => "Error::HeterogeneousList",
        Error::UnexpectedEof(_) => "Error::UnexpectedEof",
        Error::MaxDepthExceeded(_) => "Error::MaxDepthExceeded",
        Error::InvalidVarint(_) => "Error::InvalidVarint",
        Error::StringTooLong(_) => "Error::StringTooLong",
        Error::UnknownField(_) => "Error::UnknownField",
        Error::MissingVariantAs(_) => "Error::MissingVariantAs",
        Error::DiscriminantOutOfRange(_) => "Error::DiscriminantOutOfRange",
        Error::InvalidLenientWidth(_) => "Error::InvalidLenientWidth",
        Error::LenientWidthOutOfRange(_) => "Error::LenientWidthOutOfRange",
        Error::Other(_) => "Error::Other",
        // Only present when the `snbt` feature is on; the textual codec has its
        // own parse errors.
        #[cfg(feature = "snbt")]
        Error::UnexpectedSymbol(_) => "Error::UnexpectedSymbol",
        #[cfg(feature = "snbt")]
        Error::ParseIntError(_) => "Error::ParseIntError",
        #[cfg(feature = "snbt")]
        Error::ParseFloatError(_) => "Error::ParseFloatError",
    }
}

/// Hand-assembled big-endian NBT: `depth` lists, each holding one list, with an
/// empty list at the bottom. Roughly five bytes per level, which is what makes
/// unbounded nesting such a cheap attack.
fn nested_lists(depth: usize) -> Vec<u8> {
    let mut out = vec![9, 0, 0];
    for _ in 0..depth {
        out.push(9);
        out.extend_from_slice(&1_i32.to_be_bytes());
    }
    out.push(0);
    out.extend_from_slice(&0_i32.to_be_bytes());
    out
}
