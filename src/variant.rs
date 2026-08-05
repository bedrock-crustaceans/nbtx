//! NBT endianness variants and the sealed [`EndiannessImpl`] trait that maps a
//! type-level endianness marker to its runtime [`Variant`].

mod private {
    use byteorder::{BigEndian, LittleEndian};

    use super::{EndiannessImpl, Variant, VarintEndian};

    /// Prevents [`EndiannessImpl`](super::EndiannessImpl) from being implemented
    /// for types outside of this crate.
    pub trait Sealed {}

    impl Sealed for BigEndian {}
    impl EndiannessImpl for BigEndian {
        const AS_ENUM: Variant = Variant::BigEndian;
    }

    impl Sealed for LittleEndian {}
    impl EndiannessImpl for LittleEndian {
        const AS_ENUM: Variant = Variant::LittleEndian;
    }

    impl Sealed for VarintEndian {}
    impl EndiannessImpl for VarintEndian {
        const AS_ENUM: Variant = Variant::VarintEndian;
    }
}

/// Implemented by all NBT variants.
pub trait EndiannessImpl: private::Sealed {
    /// Used to convert a variant to an enum.
    /// This is used to match generic types in order to prevent having to
    /// duplicate all serialisation and deserialisation code three times.
    const AS_ENUM: Variant;
}

/// NBT format variant.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Variant {
    /// Every data type is written in little-endian format.
    /// Used by Bedrock for data saved to disk.
    LittleEndian,
    /// Every data type is written in big-endian format.
    BigEndian,
    /// The same as [`LittleEndian`](Self::LittleEndian), except that every
    /// length prefix is a varint instead of a fixed-width integer (a `u16` for
    /// strings, an `i32` for lists and arrays), and the integer and long types
    /// are also varints.
    /// Used by Bedrock for NBT transferred over the network.
    VarintEndian,
}

/// The varint little-endian NBT variant.
/// The same as [`LittleEndian`](byteorder::LittleEndian), except that every
/// length prefix is a varint instead of a fixed-width integer (a `u16` for
/// strings, an `i32` for lists and arrays), and the integer and long types are
/// also varints.
/// Used by Bedrock for NBT transferred over the network.
pub enum VarintEndian {}
