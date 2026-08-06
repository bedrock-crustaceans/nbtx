use thiserror::Error;

use crate::FieldType;

/// Convenient type definition for `Result<T, nbtx::Error>`.
pub type Result<T> = std::result::Result<T, Error>;

/// The type tag was out of range.
#[cfg(feature = "nbt")]
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
    /// The name of the field being serialised/deserialised, or `"unknown"`
    /// where the codec has no field context to attach (currently every site
    /// but the document root).
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in the buffer/string where this error occurred. No codec path
    /// tracks a position yet, so this is always `None`.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

#[cfg(feature = "nbt")]
impl TypeOutOfRange {
    /// The type that the deserializer found.
    #[inline]
    pub fn found(&self) -> u8 {
        self.found
    }

    /// The struct field at which the error occurred, or `"unknown"` when the
    /// codec had no field context to attach.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string that the error occurred at. Always
    /// `None`: no codec path records a position yet.
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
    error("expected tag of type {expected}, found {actual} at field `{at}`")
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
    /// The name of the field being serialised/deserialised, or `"unknown"`
    /// where the codec has no field context to attach (currently every site
    /// but the document root).
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in the buffer/string where this error occurred. No codec path
    /// tracks a position yet, so this is always `None`.
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

    /// The struct field at which the error occurred, or `"unknown"` when the
    /// codec had no field context to attach.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string at which the error occurred. Always
    /// `None`: no codec path records a position yet.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn index(&self) -> &Option<usize> {
        &self.index
    }
}

/// The deserializer found an [`End`] tag that was unexpected.
///
/// [`End`]: crate::FieldType::End
#[cfg(feature = "nbt")]
#[derive(Error, Debug, Clone)]
#[cfg_attr(feature = "error-context", error("unexpected end tag found at `{at}`"))]
#[cfg_attr(not(feature = "error-context"), error("unexpected end tag found"))]
pub struct UnexpectedEnd {
    /// The name of the field being serialised/deserialised, or `"unknown"`
    /// where the codec has no field context to attach (currently every site
    /// but the document root).
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in the buffer/string where this error occurred. No codec path
    /// tracks a position yet, so this is always `None`.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

#[cfg(feature = "nbt")]
impl UnexpectedEnd {
    /// The struct field at which the error occurred, or `"unknown"` when the
    /// codec had no field context to attach.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string at which the error occurred. Always
    /// `None`: no codec path records a position yet.
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
    /// The name of the field being serialised/deserialised, or `"unknown"`
    /// where the codec has no field context to attach (currently every site
    /// but the document root).
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in the buffer/string where this error occurred. No codec path
    /// tracks a position yet, so this is always `None`.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

impl Unsupported {
    /// Returns the description of the unsupported operation.
    #[inline]
    pub fn operation(&self) -> &str {
        self.op
    }

    /// The struct field at which the error occurred, or `"unknown"` when the
    /// codec had no field context to attach.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string at which the error occurred. Always
    /// `None`: no codec path records a position yet.
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
    /// The name of the field being serialised/deserialised, or `"unknown"`
    /// where the codec has no field context to attach (currently every site
    /// but the document root).
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in the buffer/string where this error occurred. No codec path
    /// tracks a position yet, so this is always `None`.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

impl UnexpectedEof {
    /// The struct field at which the error occurred, or `"unknown"` when the
    /// codec had no field context to attach.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string at which the error occurred. Always
    /// `None`: no codec path records a position yet.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn index(&self) -> &Option<usize> {
        &self.index
    }
}

/// The document nested containers more deeply than [`MAX_DEPTH`] allows.
///
/// [`MAX_DEPTH`]: crate::MAX_DEPTH
#[derive(Error, Debug, Clone)]
#[cfg_attr(
    feature = "error-context",
    error("maximum NBT nesting depth of {max} exceeded at `{at}`")
)]
#[cfg_attr(
    not(feature = "error-context"),
    error("maximum NBT nesting depth of {max} exceeded")
)]
pub struct MaxDepthExceeded {
    /// The nesting depth limit that was hit.
    pub(crate) max: usize,
    /// The name of the field being serialised/deserialised, or `"unknown"`
    /// where the codec has no field context to attach (currently every site
    /// but the document root).
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in the buffer/string where this error occurred. No codec path
    /// tracks a position yet, so this is always `None`.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

impl MaxDepthExceeded {
    /// The nesting depth limit that was hit.
    #[inline]
    pub fn max(&self) -> usize {
        self.max
    }

    /// The struct field at which the error occurred, or `"unknown"` when the
    /// codec had no field context to attach.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string at which the error occurred. Always
    /// `None`: no codec path records a position yet.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn index(&self) -> &Option<usize> {
        &self.index
    }
}

/// A varint length/integer prefix did not terminate within its maximum byte
/// count (5 bytes for a 32-bit varint, 10 for a 64-bit one).
#[cfg(feature = "nbt")]
#[derive(Error, Debug, Clone)]
#[cfg_attr(
    feature = "error-context",
    error("varint did not terminate after {max_bytes} bytes at `{at}`")
)]
#[cfg_attr(
    not(feature = "error-context"),
    error("varint did not terminate after {max_bytes} bytes")
)]
pub struct InvalidVarint {
    /// The maximum number of bytes this varint was allowed to occupy.
    pub(crate) max_bytes: usize,
    /// The name of the field being serialised/deserialised, or `"unknown"`
    /// where the codec has no field context to attach (currently every site
    /// but the document root).
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in the buffer/string where this error occurred. No codec path
    /// tracks a position yet, so this is always `None`.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

#[cfg(feature = "nbt")]
impl InvalidVarint {
    /// The maximum number of bytes this varint was allowed to occupy.
    #[inline]
    pub fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    /// The struct field at which the error occurred, or `"unknown"` when the
    /// codec had no field context to attach.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string at which the error occurred. Always
    /// `None`: no codec path records a position yet.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn index(&self) -> &Option<usize> {
        &self.index
    }
}

/// An NBT string (a `String` tag payload, a compound key or a root name) was
/// longer than [`MAX_STRING_LEN`] bytes.
///
/// [`MAX_STRING_LEN`]: crate::MAX_STRING_LEN
#[cfg(feature = "nbt")]
#[derive(Error, Debug, Clone)]
#[cfg_attr(
    feature = "error-context",
    error("NBT string of {len} bytes exceeds the maximum of {max} at `{at}`")
)]
#[cfg_attr(
    not(feature = "error-context"),
    error("NBT string of {len} bytes exceeds the maximum of {max}")
)]
pub struct StringTooLong {
    /// The length that was rejected.
    pub(crate) len: usize,
    /// The maximum permitted length.
    pub(crate) max: usize,
    /// The name of the field being serialised/deserialised, or `"unknown"`
    /// where the codec has no field context to attach (currently every site
    /// but the document root).
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in the buffer/string where this error occurred. No codec path
    /// tracks a position yet, so this is always `None`.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

#[cfg(feature = "nbt")]
impl StringTooLong {
    /// The length that was rejected.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the rejected length was zero (never the case in
    /// practice; present only to satisfy the `len`/`is_empty` convention).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The maximum permitted length.
    #[inline]
    pub fn max(&self) -> usize {
        self.max
    }

    /// The struct field at which the error occurred, or `"unknown"` when the
    /// codec had no field context to attach.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string at which the error occurred. Always
    /// `None`: no codec path records a position yet.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn index(&self) -> &Option<usize> {
        &self.index
    }
}

/// A compound key was found that the target struct has no field for.
///
/// Unknown keys are rejected by default so that schema drift cannot silently
/// discard data. Opt a struct out with `#[facet(nbtx::allow_unknown_fields)]`.
///
/// Raised by every codec — the binary one (`nbt`), the textual one (`snbt`) and
/// [`from_value`](crate::from_value) — so it is always available.
#[derive(Error, Debug, Clone)]
#[error(
    "unknown field `{field}` while deserializing `{container}` (add `#[facet(nbtx::allow_unknown_fields)]` to skip unknown keys)"
)]
pub struct UnknownField {
    /// The compound key that did not match any field.
    pub(crate) field: String,
    /// The name of the struct being deserialised into.
    pub(crate) container: &'static str,
}

impl UnknownField {
    /// The compound key that did not match any field.
    #[inline]
    pub fn field(&self) -> &str {
        &self.field
    }

    /// The name of the struct being deserialised into.
    #[inline]
    pub fn container(&self) -> &'static str {
        self.container
    }
}

/// An enum was used with nbtx without declaring how its variants are written.
///
/// Every enum nbtx encodes or decodes must carry the container attribute
/// `#[facet(nbtx::variant_as(<mode>))]`, where `<mode>` is one of `u8`, `i8`,
/// `u16`, `i16`, `u32`, `i32`, `u64`, `i64` or `str`. There is no default: the
/// wire form of an enum is part of a document's schema, so nbtx will not guess
/// one. See [`variant_as`](crate::Attr::VariantAs) for the full description.
///
/// Raised by every codec — the binary one (`nbt`), the textual one (`snbt`) and
/// [`to_value`](crate::to_value)/[`from_value`](crate::from_value) — the first
/// time it inspects the enum's shape, so it is always available.
#[derive(Error, Debug, Clone)]
#[error(
    "enum `{container}` has no `#[facet(nbtx::variant_as(...))]` attribute; add one naming the wire form of its variants (`u8`, `i8`, `u16`, `i16`, `u32`, `i32`, `u64`, `i64` or `str`)"
)]
pub struct MissingVariantAs {
    /// The name of the enum that is missing the attribute.
    pub(crate) container: &'static str,
}

impl MissingVariantAs {
    /// The name of the enum that is missing the attribute.
    #[inline]
    pub fn container(&self) -> &'static str {
        self.container
    }
}

/// A variant's discriminant does not fit the width its enum declared with
/// `#[facet(nbtx::variant_as(<mode>))]`.
///
/// Truncating it would write a number that decodes as a *different* variant (or
/// as none at all), so the value is refused instead. Either widen the mode or
/// renumber the variant.
#[derive(Error, Debug, Clone)]
#[error(
    "discriminant {discriminant} of variant `{container}::{variant}` does not fit `#[facet(nbtx::variant_as({mode}))]`"
)]
pub struct DiscriminantOutOfRange {
    /// The name of the enum the variant belongs to.
    pub(crate) container: &'static str,
    /// The name of the variant whose discriminant was out of range.
    pub(crate) variant: &'static str,
    /// The discriminant that did not fit.
    pub(crate) discriminant: i64,
    /// The declared mode, spelled as it is written in the attribute.
    pub(crate) mode: &'static str,
}

impl DiscriminantOutOfRange {
    /// The name of the enum the variant belongs to.
    #[inline]
    pub fn container(&self) -> &'static str {
        self.container
    }

    /// The name of the variant whose discriminant was out of range.
    #[inline]
    pub fn variant(&self) -> &'static str {
        self.variant
    }

    /// The discriminant that did not fit.
    #[inline]
    pub fn discriminant(&self) -> i64 {
        self.discriminant
    }

    /// The declared mode (`"u8"`, `"i16"`, …), spelled as it is written in the
    /// attribute.
    #[inline]
    pub fn mode(&self) -> &'static str {
        self.mode
    }
}

/// An unexpected symbol was encountered by the deserializer.
#[cfg(feature = "snbt")]
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
    /// The name of the field being serialised/deserialised, or `"unknown"`
    /// where the codec has no field context to attach (currently every site
    /// but the document root).
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in the buffer/string where this error occurred. No codec path
    /// tracks a position yet, so this is always `None`.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

#[cfg(feature = "snbt")]
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

    /// The struct field at which the error occurred, or `"unknown"` when the
    /// codec had no field context to attach.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string at which the error occurred. Always
    /// `None`: no codec path records a position yet.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn index(&self) -> &Option<usize> {
        &self.index
    }
}

#[cfg(feature = "snbt")]
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
    /// The name of the field being serialised/deserialised, or `"unknown"`
    /// where the codec has no field context to attach (currently every site
    /// but the document root).
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in the buffer/string where this error occurred. No codec path
    /// tracks a position yet, so this is always `None`.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

#[cfg(feature = "snbt")]
impl ParseIntError {
    /// Returns the actual parsing error.
    #[inline]
    pub fn error(&self) -> &std::num::ParseIntError {
        &self.error
    }

    /// The struct field at which the error occurred, or `"unknown"` when the
    /// codec had no field context to attach.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string at which the error occurred. Always
    /// `None`: no codec path records a position yet.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn index(&self) -> &Option<usize> {
        &self.index
    }
}

#[cfg(feature = "snbt")]
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
    /// The name of the field being serialised/deserialised, or `"unknown"`
    /// where the codec has no field context to attach (currently every site
    /// but the document root).
    #[cfg(feature = "error-context")]
    pub(crate) at: String,
    /// The index in the buffer/string where this error occurred. No codec path
    /// tracks a position yet, so this is always `None`.
    #[cfg(feature = "error-context")]
    pub(crate) index: Option<usize>,
}

#[cfg(feature = "snbt")]
impl ParseFloatError {
    /// Returns the actual parsing error.
    #[inline]
    pub fn error(&self) -> &std::num::ParseFloatError {
        &self.error
    }

    /// The struct field at which the error occurred, or `"unknown"` when the
    /// codec had no field context to attach.
    #[cfg(feature = "error-context")]
    #[inline]
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The index into the buffer/string at which the error occurred. Always
    /// `None`: no codec path records a position yet.
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
    #[cfg(feature = "nbt")]
    #[error(transparent)]
    TypeOutOfRange(TypeOutOfRange),
    /// Found a type different from the type that was expected.
    #[error(transparent)]
    UnexpectedType(UnexpectedType),
    #[cfg(feature = "nbt")]
    #[error(transparent)]
    UnexpectedEnd(UnexpectedEnd),
    /// The requested operation is not supported.
    #[error(transparent)]
    Unsupported(Unsupported),
    #[error("{0}")]
    Other(String),
    /// An NBT `List` tag contained elements of more than one tag type.
    ///
    /// The wire format stores a single element-type byte for the whole list, so
    /// a [`Value::List`](crate::Value::List) whose elements do not all share the
    /// first element's tag cannot be encoded without silently corrupting the
    /// stream.
    #[error("heterogeneous NBT list: every element must be {expected}, found {found}")]
    HeterogeneousList {
        /// The tag of the first (reference) element.
        expected: FieldType,
        /// The first element tag that differed.
        found: FieldType,
    },
    #[error(transparent)]
    UnexpectedEof(UnexpectedEof),
    /// The document nested containers deeper than [`MAX_DEPTH`](crate::MAX_DEPTH).
    ///
    /// Raised by every codec — the binary one (`nbt`), the textual one (`snbt`)
    /// and the [`Value`](crate::Value) conversion — on read and on write.
    #[error(transparent)]
    MaxDepthExceeded(MaxDepthExceeded),
    /// A varint did not terminate within its permitted byte count.
    #[cfg(feature = "nbt")]
    #[error(transparent)]
    InvalidVarint(InvalidVarint),
    /// An NBT string exceeded [`MAX_STRING_LEN`](crate::MAX_STRING_LEN) bytes.
    #[cfg(feature = "nbt")]
    #[error(transparent)]
    StringTooLong(StringTooLong),
    /// A compound key had no matching struct field.
    ///
    /// Raised by every codec (binary, SNBT and [`from_value`](crate::from_value)).
    #[error(transparent)]
    UnknownField(UnknownField),
    /// An enum did not declare `#[facet(nbtx::variant_as(...))]`.
    ///
    /// Raised by every codec (binary, SNBT and the [`Value`](crate::Value)
    /// conversion), on read and on write.
    #[error(transparent)]
    MissingVariantAs(MissingVariantAs),
    /// A variant's discriminant did not fit the declared `variant_as` width.
    #[error(transparent)]
    DiscriminantOutOfRange(DiscriminantOutOfRange),
    #[cfg(feature = "snbt")]
    #[error(transparent)]
    UnexpectedSymbol(UnexpectedSymbol),
    #[cfg(feature = "snbt")]
    #[error(transparent)]
    ParseIntError(ParseIntError),
    #[cfg(feature = "snbt")]
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
