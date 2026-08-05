//! The NBT tag-type byte ([`FieldType`]) and its conversions.

use std::fmt::{self, Display};

#[cfg(feature = "nbt")]
use crate::error::TypeOutOfRange;
#[cfg(feature = "nbt")]
use crate::{Error, Result};

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
    /// A string of raw bytes; not guaranteed to be valid UTF-8.
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

#[cfg(feature = "nbt")]
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
