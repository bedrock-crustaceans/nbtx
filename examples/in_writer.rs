//! Embedding NBT inside a larger byte stream, in both directions.
//!
//! `cargo run --example in_writer`
//!
//! NBT is rarely a whole file. In the Bedrock protocol it is one field of a
//! packet, sandwiched between a header and whatever follows; in a world file it
//! sits inside a chunk record. So the encoder writes into any `io::Write` and
//! the decoder reads from any `io::Read`, consuming exactly the bytes of one
//! document and leaving the cursor on the next byte — no length prefix and no
//! separate framing needed, because an NBT document is self-delimiting.

use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use byteorder::{BigEndian, LittleEndian, ReadBytesExt, WriteBytesExt};
use facet::Facet;
use nbtx::{Serializer, VarintEndian};

/// A made-up packet: a fixed header, one NBT payload, then a trailer.
#[derive(Facet, Debug, PartialEq)]
struct BlockUpdate {
    block: String,
    x: i32,
    y: i32,
    z: i32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let update = BlockUpdate {
        block: "minecraft:redstone_torch".to_owned(),
        x: 128,
        y: 64,
        z: -32,
    };

    let mut packet = Vec::new();

    packet.write_u8(0x2f)?;
    packet.write_u16::<LittleEndian>(0xbeef)?;
    let nbt_start = packet.len();

    // `to_*_bytes_in` appends to a writer you already own instead of returning
    // a fresh `Vec`, which avoids a copy and lets NBT sit anywhere in a buffer.
    // Bedrock's on-disk variant is little-endian, so that is the one used here.
    nbtx::to_le_bytes_in(&mut packet, &update)?;
    let nbt_end = packet.len();

    packet.write_all(b"<trailer>")?;

    println!("packet layout:");
    println!("  header    bytes 0..{nbt_start}");
    println!("  NBT       bytes {nbt_start}..{nbt_end}");
    println!("  trailer   bytes {nbt_end}..{}", packet.len());

    // Reading it back. The decoder stops at the end of the document on its own,
    // so the trailer is still there for whoever parses it next.
    let mut cursor = Cursor::new(packet.as_slice());
    let kind = cursor.read_u8()?;
    let sequence = cursor.read_u16::<LittleEndian>()?;
    let decoded: BlockUpdate = nbtx::from_le_bytes(&mut cursor)?;
    println!("\nread header: kind {kind:#04x}, sequence {sequence:#06x}");
    println!("read NBT:    {decoded:?}");
    println!(
        "cursor now at byte {} of {}",
        cursor.position(),
        packet.len()
    );
    assert_eq!(decoded, update);
    assert_eq!(cursor.position() as usize, nbt_end);

    let mut trailer = String::new();
    cursor.read_to_string(&mut trailer)?;
    println!("read trailer: {trailer:?}");

    // Several documents can follow one another with nothing in between, since
    // each one ends where its root tag ends.
    let mut stream = Vec::new();
    for i in 0..3 {
        nbtx::to_le_bytes_in(
            &mut stream,
            &BlockUpdate {
                block: format!("minecraft:block_{i}"),
                x: i,
                y: 0,
                z: 0,
            },
        )?;
    }
    let mut reader = Cursor::new(stream.as_slice());
    println!("\nthree back-to-back documents in {} bytes:", stream.len());
    while reader.position() < stream.len() as u64 {
        let one: BlockUpdate = nbtx::from_le_bytes(&mut reader)?;
        println!("  at byte {:>3}: {}", reader.position(), one.block);
    }

    // `Serializer` is the same writer with the endianness fixed as a type
    // parameter rather than chosen per call. It is worth reaching for when a
    // function is generic over the variant, or when the writer is owned by the
    // serializer for a while — `into_inner` hands it back.
    let mut ser: Serializer<Vec<u8>, VarintEndian> = Serializer::new(Vec::new());
    ser.serialize(&update)?;
    let varint = ser.into_inner();
    println!(
        "\nvia `Serializer<_, VarintEndian>`: {} bytes",
        varint.len()
    );
    let back: BlockUpdate = nbtx::from_varint_bytes(&mut varint.as_slice())?;
    assert_eq!(back, update);

    // Any `Write` works, not just a `Vec` — a file, a socket, a compressor.
    // Here an in-memory cursor stands in for a file so the example writes
    // nothing to disk: seek to an offset, overwrite a region, and the NBT lands
    // exactly there.
    let mut file = Cursor::new(vec![0_u8; 64]);
    file.seek(SeekFrom::Start(16))?;
    nbtx::to_bytes_in::<BigEndian>(&mut file, &nbtx::Value::Int(7))?;
    let written = file.position() as usize - 16;
    println!("wrote {written} bytes at offset 16 of a seekable target");

    Ok(())
}
