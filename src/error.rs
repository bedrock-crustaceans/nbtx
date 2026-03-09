use std::borrow::Cow;

use thiserror::Error;

use crate::FieldType;

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
        value: String,
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
        found: char,
        expected: Option<char>,
        /// The name of the field being serialised/deserialised.
        at: String,
        /// The index in in the buffer/string where this error occurred.
        /// This is none when serialising.
        index: Option<usize>,
    },
    #[error("failed to parse int: \"{error}\" at `{at}`")]
    ParseIntError {
        error: std::num::ParseIntError,
        /// The name of the field being serialised/deserialised.
        at: String,
        /// The index in in the buffer/string where this error occurred.
        /// This is none when serialising.
        index: Option<usize>,
    },
    #[error("failed to parse float: \"{error}\" at `{at}`")]
    ParseFloatError {
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

/// Errors related to binary reading and writing.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum StreamError {
    // TODO: std::io::Error does not implement Clone while the ProtoCodec error type requires it.
    // This is why I convert the error to a string rather than storing it directly like the others.
    /// An IO [`Error`](std::io::Error).
    #[error("{0}")]
    IoError(String),
    /// A byte slice could not be converted into a `String` because it is invalid UTF-8.
    #[error("{0}")]
    FromUtf8Error(#[from] std::string::FromUtf8Error),
    /// A byte slice could not be converted into a `str` because it is invalid UTF-8.
    #[error("{0}")]
    Utf8Error(#[from] std::str::Utf8Error),
    /// The deserializer tried to read past the end of the buffer.
    #[error("Expected {expected} remaining bytes, found only {remaining}")]
    UnexpectedEof { expected: usize, remaining: usize },
    /// Any errors that do not fit the previous categories.
    #[error("{0}")]
    Other(Cow<'static, str>),
}

impl From<std::io::Error> for StreamError {
    fn from(value: std::io::Error) -> Self {
        Self::IoError(value.to_string())
    }
}
