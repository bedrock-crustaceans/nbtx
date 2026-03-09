use thiserror::Error;

use crate::FieldType;

/// Convenient type definition for `Result<T, nbtx::Error>`.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur while serializing or deserializing NBT data.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The encountered NBT tag type is invalid.
    #[error(
        "unknown tag type was encountered `{found:#0x}` at `{at}`, it should be in the range 0-12"
    )]
    TypeOutOfRange {
        /// The found type
        found: u8,
        /// The name of the field being serialised/deserialised.
        at: String,
        /// The index in in the buffer/string where this error occurred.
        /// This is none when serialising.
        index: Option<usize>,
    },
    /// Found a type different from the type that was expected.
    #[error("expected tag of type {expected}, received {actual} at field `{at}`)")]
    UnexpectedType {
        /// Type that the deserializer was expecting to find.
        expected: FieldType,
        /// Type that was found in the NBT stream.
        actual: FieldType,
        /// The name of the field being serialised/deserialised.
        at: String,
        /// The index in in the buffer/string where this error occurred.
        /// This is none when serialising.
        index: Option<usize>,
    },
    #[error("unexpected end tag found at `{at}`")]
    UnexpectedEnd {
        /// The name of the field being serialised/deserialised.
        at: String,
        /// The index in in the buffer/string where this error occurred.
        /// This is none when serialising.
        index: Option<usize>,
    },
    /// The requested operation is not supported.
    #[error("`{op}` at field `{at}`")]
    Unsupported {
        /// Description of the error
        op: &'static str,
        /// The name of the field being serialised/deserialised.
        at: String,
        /// The index in in the buffer/string where this error occurred.
        /// This is none when serialising.
        index: Option<usize>,
    },
    #[error("expected a valid number at `{at}`")]
    ExpectedInteger {
        /// The name of the field being serialised/deserialised.
        at: String,
        /// The index in in the buffer/string where this error occurred.
        /// This is none when serialising.
        index: Option<usize>,
    },
    /// Integer is too large to fit in the given type
    #[error("integer `{value}` is too large for type {ty} at `{at}`")]
    IntegerTooLarge {
        /// The value that was read.
        value: String,
        /// The type of integer that the deserialiser attempted to read.
        ty: FieldType,
        /// The name of the field being serialised/deserialised.
        at: String,
        /// The index in in the buffer/string where this error occurred.
        /// This is none when serialising.
        index: Option<usize>,
    },
    #[error("{0}")]
    Other(String),
    #[error("unexpected end of file at `{at}`")]
    Eof {
        /// The name of the field being serialised/deserialised.
        at: String,
        /// The index in in the buffer/string where this error occurred.
        /// This is none when serialising.
        index: Option<usize>,
    },
    #[error("encountered unexpected symbol '{found}', at `{at}`")]
    ExpectedSymbol {
        /// The symbol that was found.
        found: char,
        /// The symbol that the deserialiser expected, or `None` if it had no specific expectations.
        expected: Option<char>,
        /// The name of the field being serialised/deserialised.
        at: String,
        /// The index in in the buffer/string where this error occurred.
        /// This is none when serialising.
        index: Option<usize>,
    },
    #[error("failed to parse int: \"{error}\" at `{at}`")]
    ParseIntError {
        /// The parsing error itself.
        error: std::num::ParseIntError,
        /// The name of the field being serialised/deserialised.
        at: String,
        /// The index in in the buffer/string where this error occurred.
        /// This is none when serialising.
        index: Option<usize>,
    },
    #[error("failed to parse float: \"{error}\" at `{at}`")]
    ParseFloatError {
        /// The parsing error itself.
        error: std::num::ParseFloatError,
        /// The name of the field being serialised/deserialised.
        at: String,
        /// The index in in the buffer/string where this error occurred.
        /// This is none when serialising.
        index: Option<usize>,
    },
}

impl From<std::string::FromUtf8Error> for Error {
    fn from(value: std::string::FromUtf8Error) -> Error {
        Error::Other(value.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Error {
        if value.kind() == std::io::ErrorKind::UnexpectedEof {
            // TODO: How should I retrieve the current field name?
            Error::Eof {
                at: "unknown".to_string(),
                index: None,
            }
        } else {
            Error::Other(value.to_string())
        }
    }
}