use std::borrow::Cow;

use thiserror::Error;

use crate::FieldType;

pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur while serializing or deserializing NBT data.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The encountered NBT tag type is invalid.
    #[error(
        "unknown tag type was encountered ({actual}) at `{at}`, it should be in the range 0-12"
    )]
    TypeOutOfRange { actual: u8, at: String },
    /// Found a type different from the type that was expected.
    #[error("expected tag of type {expected}, received {actual} at field `{at}`)")]
    UnexpectedType {
        /// Type that the deserializer was expecting to find.
        expected: FieldType,
        /// Type that was found in the NBT stream.
        actual: FieldType,
        /// The struct field that the error occurred at.
        /// This is provided on a best-effort basis and may not always be accurate.
        at: String,
    },
    /// Any errors related to reading and writing from the stream.
    #[error("{0}")]
    ByteError(#[from] StreamError),
    #[error("unexpected end tag")]
    UnexpectedEnd,
    /// The requested operation is not supported.
    #[error("`{op}` at field `{at}`")]
    Unsupported {
        /// Description of the error
        op: &'static str,
        /// The struct field that the error ocurred at.
        at: String,
    },
    #[error("expected a valid number at `{0}`")]
    ExpectedInteger(String),
    /// Integer is too large to fit in the given type
    #[error("integer `{value}` is too large for type {ty} at `{at}`")]
    IntegerTooLarge {
        value: String,
        ty: FieldType,
        at: String,
    },
    #[error("{0}")]
    Other(String),
    // #[error("{0}")]
    // Malformed(&'static str),
    #[error("unexpected end of file at `{0}`")]
    Eof(String),
    #[error("expected '{expected}', found '{found}', at `{at}`")]
    ExpectedSymbol {
        found: char,
        expected: char,
        at: String,
    },
    #[error("failed to parse int: \"{error}\" at `{at}`")]
    ParseIntError {
        error: std::num::ParseIntError,
        at: String,
    },
    #[error("failed to parse float: \"{error}\" at `{at}`")]
    ParseFloatError {
        error: std::num::ParseFloatError,
        at: String,
    },
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::ByteError(StreamError::IoError(value.to_string()))
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(value: std::str::Utf8Error) -> Self {
        Self::ByteError(StreamError::Utf8Error(value))
    }
}

impl From<std::string::FromUtf8Error> for Error {
    fn from(value: std::string::FromUtf8Error) -> Self {
        Self::ByteError(StreamError::FromUtf8Error(value))
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
