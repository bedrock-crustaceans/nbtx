use thiserror::Error;

use crate::FieldType;

/// Convenient type definition for `Result<T, nbtx::Error>`.
pub type Result<T> = std::result::Result<T, Error>;

/// The type tag was out of range.
#[derive(Error, Debug, Clone)]
#[cfg_attr(
    feature = "error-context",
    error(
        "unknown tag type was encountered `{found:#0x}` at `{at}`, it should be in the range 0-12"
    )
)]
#[cfg_attr(
    not(feature = "error-context"),
    error("unknown tag type was encountered `{found:#0x}`, should be in the range 0-12",)
)]
pub struct TypeOutOfRange {
    /// The found type
    pub(crate) found: u8,
    /// The name of the field being serialised/deserialised.
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in in the buffer/string where this error occurred.
    /// This is none when serialising.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

impl TypeOutOfRange {
    /// The type that the deserializer found.
    #[inline]
    pub fn found(&self) -> u8 {
        self.found
    }

    /// The struct field at which the error occurred.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into into the buffer/string that the error occurred at.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn index(&self) -> &Option<usize> {
        &self.index
    }
}

/// The deserializer found a tag type that was unexpected.
#[derive(Error, Debug, Clone)]
#[cfg_attr(
    feature = "error-context",
    error("expected tag of type {expected}, found {actual} at field `{at}`)")
)]
#[cfg_attr(
    not(feature = "error-context"),
    error("expected tag of type {expected}, found {actual}")
)]
pub struct UnexpectedType {
    /// Type that the deserializer was expecting to find.
    pub(crate) expected: FieldType,
    /// Type that was found in the NBT stream.
    pub(crate) actual: FieldType,
    /// The name of the field being serialised/deserialised.
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in in the buffer/string where this error occurred.
    /// This is none when serialising.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

impl UnexpectedType {
    /// The type that the deserializer expected to find.
    #[inline]
    pub fn expected(&self) -> FieldType {
        self.expected
    }

    /// The type that the deserializer actually found.
    #[inline]
    pub fn found(&self) -> FieldType {
        self.actual
    }

    /// The struct field at which the error occurred.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string at which the error occurred.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn index(&self) -> &Option<usize> {
        &self.index
    }
}

/// The deserializer found an [`End`] tag that was unexpected.
///
/// [`End`]: crate::FieldType::End
#[derive(Error, Debug, Clone)]
#[cfg_attr(feature = "error-context", error("unexpected end tag found at `{at}`"))]
#[cfg_attr(not(feature = "error-context"), error("unexpected end tag found"))]
pub struct UnexpectedEnd {
    /// The name of the field being serialised/deserialised.
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in in the buffer/string where this error occurred.
    /// This is none when serialising.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

impl UnexpectedEnd {
    /// The struct field at which the error occurred.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string at which the error occurred.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn index(&self) -> &Option<usize> {
        &self.index
    }
}

/// The attempted operation is unsupported.
#[derive(Error, Debug, Clone)]
#[cfg_attr(feature = "error-context", error("`{op}`, at field `{at}`"))]
#[cfg_attr(not(feature = "error-context"), error("`{op}`"))]
pub struct Unsupported {
    /// Description of the error
    pub(crate) op: &'static str,
    /// The name of the field being serialised/deserialised.
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in in the buffer/string where this error occurred.
    /// This is none when serialising.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

impl Unsupported {
    /// Returns the description of the unsupported operation.
    #[inline]
    pub fn operation(&self) -> &str {
        self.op
    }

    /// The struct field at which the error occurred.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string at which the error occurred.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn index(&self) -> &Option<usize> {
        &self.index
    }
}

/// The deserializer expected a number but did not find it.
#[derive(Error, Debug, Clone)]
#[cfg_attr(feature = "error-context", error("expected a valid number at `{at}`"))]
#[cfg_attr(not(feature = "error-context"), error("expected a valid number"))]
pub struct ExpectedNumber {
    /// The name of the field being serialised/deserialised.
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in in the buffer/string where this error occurred.
    /// This is none when serialising.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

impl ExpectedNumber {
    /// The struct field at which the error occurred.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string at which the error occurred.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn index(&self) -> &Option<usize> {
        &self.index
    }
}

/// The integer that the deserializer tried to parse was too large for its type.
#[derive(Error, Debug, Clone)]
#[cfg_attr(
    feature = "error-context",
    error("integer `{value}` is too large for type {ty} at `{at}`")
)]
#[cfg_attr(
    not(feature = "error-context"),
    error("integer `{value}` is too large for type {ty}")
)]
pub struct IntegerTooLarge {
    /// The value that was read.
    pub(crate) value: String,
    /// The type of integer that the deserialiser attempted to read.
    pub(crate) ty: FieldType,
    /// The name of the field being serialised/deserialised.
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in in the buffer/string where this error occurred.
    /// This is none when serialising.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

impl IntegerTooLarge {
    /// Returns the string value that was read.
    #[inline]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// The type of integer that the deserializer was trying to parse.
    #[inline]
    pub fn ty(&self) -> FieldType {
        self.ty
    }

    /// The struct field at which the error occurred.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string at which the error occurred.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn index(&self) -> &Option<usize> {
        &self.index
    }
}

/// The input unexpectedly ended.
#[derive(Error, Debug, Clone)]
#[cfg_attr(feature = "error-context", error("unexpected end of file at `{at}`"))]
#[cfg_attr(not(feature = "error-context"), error("unexpected end of file"))]
pub struct UnexpectedEof {
    /// The name of the field being serialised/deserialised.
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in in the buffer/string where this error occurred.
    /// This is none when serialising.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

impl UnexpectedEof {
    /// The struct field at which the error occurred.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string at which the error occurred.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn index(&self) -> &Option<usize> {
        &self.index
    }
}

/// An unexpected symbol was encountered by the deserializer.
#[derive(Error, Debug, Clone)]
#[cfg_attr(
    feature = "error-context",
    error("encountered unexpected symbol '{found}', at `{at}`")
)]
#[cfg_attr(
    not(feature = "error-context"),
    error("encountered unexpected symbol '{found}'")
)]
pub struct UnexpectedSymbol {
    /// The symbol that was found.
    pub(crate) found: char,
    /// The symbol that the deserialiser expected, or `None` if it had no specific expectations.
    pub(crate) expected: Option<char>,
    /// The name of the field being serialised/deserialised.
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in in the buffer/string where this error occurred.
    /// This is none when serialising.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

impl UnexpectedSymbol {
    /// The symbol that the deserializer found.
    #[inline]
    pub fn found(&self) -> char {
        self.found
    }

    /// The symbol that the deserializer expected, or `None` if it had no specific expectations.
    #[inline]
    pub fn expected(&self) -> Option<char> {
        self.expected
    }

    /// The struct field at which the error occurred.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string at which the error occurred.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn index(&self) -> &Option<usize> {
        &self.index
    }
}

#[derive(Error, Debug, Clone)]
#[cfg_attr(
    feature = "error-context",
    error("failed to parse int: \"{error}\" at `{at}`")
)]
#[cfg_attr(
    not(feature = "error-context"),
    error("failed to parse int: \"{error}\"")
)]
pub struct ParseIntError {
    /// The parsing error itself.
    pub(crate) error: std::num::ParseIntError,
    /// The name of the field being serialised/deserialised.
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in in the buffer/string where this error occurred.
    /// This is none when serialising.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

impl ParseIntError {
    /// Returns the actual parsing error.
    #[inline]
    pub fn error(&self) -> &std::num::ParseIntError {
        &self.error
    }

    /// The struct field at which the error occurred.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string at which the error occurred.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn index(&self) -> &Option<usize> {
        &self.index
    }
}

#[derive(Error, Debug, Clone)]
#[cfg_attr(
    feature = "error-context",
    error("failed to parse float: \"{error}\" at `{at}`")
)]
#[cfg_attr(
    not(feature = "error-context"),
    error("failed to parse float: \"{error}\"")
)]
pub struct ParseFloatError {
    /// The parsing error itself.
    pub(crate) error: std::num::ParseFloatError,
    /// The name of the field being serialised/deserialised.
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in in the buffer/string where this error occurred.
    /// This is none when serialising.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

impl ParseFloatError {
    /// Returns the actual parsing error.
    #[inline]
    pub fn error(&self) -> &std::num::ParseFloatError {
        &self.error
    }

    /// The struct field at which the error occurred.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string at which the error occurred.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn index(&self) -> &Option<usize> {
        &self.index
    }
}

/// Errors that can occur while serializing or deserializing NBT data.
#[derive(Error, Debug, Clone)]
pub enum Error {
    /// The encountered NBT tag type is invalid.
    #[error(transparent)]
    TypeOutOfRange(TypeOutOfRange),
    /// Found a type different from the type that was expected.
    #[error(transparent)]
    UnexpectedType(UnexpectedType),
    #[error(transparent)]
    UnexpectedEnd(UnexpectedEnd),
    /// The requested operation is not supported.
    #[error(transparent)]
    Unsupported(Unsupported),
    /// The deserializer expected a number but found something else.
    #[error(transparent)]
    ExpectedNumber(ExpectedNumber),
    /// Integer is too large to fit in the given type
    #[error(transparent)]
    IntegerTooLarge(IntegerTooLarge),
    #[error("{0}")]
    Other(String),
    #[error(transparent)]
    UnexpectedEof(UnexpectedEof),
    #[error(transparent)]
    UnexpectedSymbol(UnexpectedSymbol),
    #[error(transparent)]
    ParseIntError(ParseIntError),
    #[error(transparent)]
    ParseFloatError(ParseFloatError),
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
            Error::UnexpectedEof(UnexpectedEof {
                #[cfg(feature = "error-context")]
                at: "unknown".to_string(),
                #[cfg(feature = "error-context")]
                index: None,
            })
        } else {
            Error::Other(value.to_string())
        }
    }
}
