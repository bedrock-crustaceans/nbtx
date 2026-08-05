//! The three NBT wire variants and when to use which.
//!
//! `cargo run --example endianness`
//!
//! NBT is one tag model with three byte layouts. nbtx exposes each as a pair of
//! free functions, and the variant is *not* recorded in the file — you have to
//! know which one you are holding, exactly as Minecraft itself does.
//!
//! | Variant | Encode | Decode | Used by |
//! | --- | --- | --- | --- |
//! | `BigEndian` | `to_be_bytes` | `from_be_bytes` | Java Edition, most `.nbt` files |
//! | `LittleEndian` | `to_le_bytes` | `from_le_bytes` | Bedrock, on disk (`level.dat`, worlds) |
//! | `VarintEndian` | `to_varint_bytes` | `from_varint_bytes` | Bedrock, over the network |

use byteorder::{BigEndian, LittleEndian};
use facet::Facet;
use nbtx::VarintEndian;

#[derive(Facet, Debug, PartialEq)]
struct Telemetry {
    /// Small in value but 4 bytes wide in the fixed-width variants, which is
    /// exactly the case varints exist to shrink.
    tick: i32,
    entity_id: i64,
    label: String,
}

fn main() -> Result<(), nbtx::Error> {
    let sample = Telemetry {
        tick: 12,
        entity_id: 3,
        label: "spawn".to_owned(),
    };

    let be = nbtx::to_be_bytes(&sample)?;
    let le = nbtx::to_le_bytes(&sample)?;
    let varint = nbtx::to_varint_bytes(&sample)?;

    println!("the same value in all three variants:");
    println!("  big-endian     {:>3} bytes  {}", be.len(), hex(&be));
    println!("  little-endian  {:>3} bytes  {}", le.len(), hex(&le));
    println!(
        "  varint         {:>3} bytes  {}",
        varint.len(),
        hex(&varint)
    );

    // The varint variant zigzag-encodes ints, longs and every length prefix, so
    // small numbers and short strings cost one byte instead of four (or two).
    // That is why Bedrock uses it on the wire and the fixed-width variant on
    // disk, where seekability matters more than size.
    println!(
        "\nvarint saves {} bytes here ({}% smaller) by shrinking small integers",
        be.len() - varint.len(),
        100 - (varint.len() * 100 / be.len())
    );

    assert_eq!(
        nbtx::from_be_bytes::<Telemetry>(&mut be.as_slice())?,
        sample
    );
    assert_eq!(
        nbtx::from_le_bytes::<Telemetry>(&mut le.as_slice())?,
        sample
    );
    assert_eq!(
        nbtx::from_varint_bytes::<Telemetry>(&mut varint.as_slice())?,
        sample
    );
    println!("all three round-trip to the same value");

    // Each `to_*`/`from_*` pair is a thin alias over the generic entry points,
    // which take the variant as a type parameter. Reach for these when the
    // variant is itself generic in your code — a codec parameterised over
    // `E: EndiannessImpl` only has to be written once.
    let generic = nbtx::to_bytes::<LittleEndian>(&sample)?;
    assert_eq!(generic, le);
    let generic_back = nbtx::from_bytes::<LittleEndian, Telemetry>(&mut generic.as_slice())?;
    assert_eq!(generic_back, sample);
    // `BigEndian`/`LittleEndian` are byteorder's own marker types, re-exported;
    // the varint variant has no byteorder equivalent, so nbtx defines its own.
    assert_eq!(nbtx::to_bytes::<VarintEndian>(&sample)?, varint);
    println!("`to_bytes::<LittleEndian>` is the same thing as `to_le_bytes`");

    // Nothing in the stream identifies the variant, so decoding with the wrong
    // one gives garbage or, more often, an error. Both outcomes are safe — the
    // decoder never panics on bad input — but neither is recoverable, so track
    // the variant out of band.
    println!("\ndecoding with the wrong variant fails rather than silently lying:");
    match nbtx::from_bytes::<BigEndian, Telemetry>(&mut le.as_slice()) {
        Ok(wrong) => println!("  little-endian bytes read as big-endian: {wrong:?}"),
        Err(err) => println!("  little-endian bytes read as big-endian: {err}"),
    }
    match nbtx::from_bytes::<BigEndian, Telemetry>(&mut varint.as_slice()) {
        Ok(wrong) => println!("  varint bytes read as big-endian: {wrong:?}"),
        Err(err) => println!("  varint bytes read as big-endian: {err}"),
    }

    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join("")
}
