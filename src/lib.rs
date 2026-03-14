//! Implements NBT serialisation and deserialization for three different integer encodings.

#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::enum_glob_use)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
// Ensures that docs.rs builds all features and displays which feature flags to use for the types.
#![cfg_attr(docsrs, feature(doc_cfg))]

use crate::error::TypeOutOfRange;
pub use crate::nbt::de::{Deserializer, from_be_bytes, from_bytes, from_le_bytes, from_net_bytes};
pub use crate::nbt::ser::{
    Serializer, to_be_bytes, to_be_bytes_in, to_bytes, to_bytes_in, to_le_bytes, to_le_bytes_in,
    to_net_bytes, to_net_bytes_in,
};
pub use crate::value::Value;
pub use byteorder::{BigEndian, LittleEndian};

use std::fmt::{self, Debug, Display};

pub use error::{Error, Result};

#[cfg(test)]
mod test;

mod error;
mod nbt;

#[cfg(feature = "snbt")]
pub mod snbt;
#[cfg(feature = "snbt")]
pub use snbt::{from_string, to_string};

mod value;

mod private {
    use byteorder::{BigEndian, LittleEndian};

    use crate::{EndiannessImpl, NetworkLittleEndian, Variant};

    /// Prevents [`VariantImpl`](super::VariantImpl) from being implemented for
    /// types outside of this crate.
    pub trait Sealed {}

    impl Sealed for BigEndian {}
    impl EndiannessImpl for BigEndian {
        const AS_ENUM: Variant = Variant::BigEndian;
    }

    impl Sealed for LittleEndian {}
    impl EndiannessImpl for LittleEndian {
        const AS_ENUM: Variant = Variant::LittleEndian;
    }

    impl Sealed for NetworkLittleEndian {}
    impl EndiannessImpl for NetworkLittleEndian {
        const AS_ENUM: Variant = Variant::NetworkEndian;
    }
}

/// Implemented by all NBT variants.
pub trait EndiannessImpl: private::Sealed {
    /// Used to convert a variant to an enum.
    /// This is used to match generic types in order to prevent
    /// having to duplicate all deserialisation code three times.
    const AS_ENUM: Variant;
}

/// NBT format variant.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Variant {
    /// Used by Bedrock for data saved to disk.
    /// Every data type is written in little endian format.
    LittleEndian,
    /// Used by Java.
    /// Every data types is written in big endian format.
    BigEndian,
    /// Used by Bedrock for NBT transferred over the network.
    /// This format is the same as [`LittleEndian`], except that type lengths
    /// (such as for strings or lists), are varints instead of shorts.
    /// The integer and long types are also varints.
    NetworkEndian,
}

/// Used by Bedrock for NBT transferred over the network.
/// This format is the same as [`LittleEndian`], except that type lengths
/// (such as for strings or lists), are varints instead of shorts.
/// The integer and long types are also varints.
pub enum NetworkLittleEndian {}

/// NBT field type
// Compiler complains about unused enum variants even though they're constructed using a transmute.
#[allow(dead_code)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum FieldType {
    /// Indicates the end of a compound tag.
    End = 0,
    /// A signed byte.
    Byte = 1,
    /// A signed short.
    Short = 2,
    /// A signed int.
    Int = 3,
    /// A signed long.
    Long = 4,
    /// A float.
    Float = 5,
    /// A double.
    Double = 6,
    /// An array of byte tags.
    ByteArray = 7,
    /// A UTF-8 string.
    String = 8,
    /// List of tags.
    /// Every item in the list must be of the same type.
    List = 9,
    /// A key-value map.
    Compound = 10,
    /// An array of int tags.
    IntArray = 11,
    /// An array of long tags.
    LongArray = 12,
}

impl Display for FieldType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use FieldType::*;

        let str = match self {
            End => "end",
            Byte => "byte",
            Short => "short",
            Int => "int",
            Long => "long",
            Float => "float",
            Double => "double",
            ByteArray => "byte array",
            String => "string",
            List => "list",
            Compound => "compound",
            IntArray => "int array",
            LongArray => "long array",
        };

        f.write_str(str)
    }
}

impl FieldType {
    pub(crate) fn try_from(
        v: u8,
        #[cfg(feature = "error-context")] at: &mut Option<String>,
        #[cfg(feature = "error-context")] at_index: Option<usize>,
    ) -> Result<Self> {
        const LAST_DISC: u8 = FieldType::LongArray as u8;
        if v > LAST_DISC {
            return Err(Error::TypeOutOfRange(TypeOutOfRange {
                found: v,

                #[cfg(feature = "error-context")]
                at: at.take().unwrap_or_else(|| String::from("unknown")),
                #[cfg(feature = "error-context")]
                index: at_index,
            }));
        }

        // SAFETY: Because `Self` is marked as `repr(u8)`, its layout is guaranteed to start
        // with a `u8` discriminant as its first field. Additionally, the raw discriminant is verified
        // to be in the enum's range.
        Ok(unsafe { std::mem::transmute::<u8, FieldType>(v) })
    }
}

// impl TryFrom<u8> for FieldType {
//     type Error = Error;

//     fn try_from(v: u8) -> Result<Self> {
//         const LAST_DISC: u8 = FieldType::LongArray as u8;
//         if v > LAST_DISC {
//             return Err(Error::TypeOutOfRange { actual: v });
//         }

//         // SAFETY: Because `Self` is marked as `repr(u8)`, its layout is guaranteed to start
//         // with a `u8` discriminant as its first field. Additionally, the raw discriminant is verified
//         // to be in the enum's range.
//         Ok(unsafe { std::mem::transmute::<u8, FieldType>(v) })
//     }
// }

impl serde::de::Error for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: Display,
    {
        Error::Other(msg.to_string())
    }
}

impl serde::ser::Error for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: Display,
    {
        Error::Other(msg.to_string())
    }
}
