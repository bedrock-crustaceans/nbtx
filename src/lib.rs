//! Implements NBT serialisation and deserialization for three different integer encodings.

#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::enum_glob_use)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
// The public error accessors intentionally return `&Option<T>` for a stable API.
#![allow(clippy::ref_option)]
// Ensures that docs.rs builds all features and displays which feature flags to use for the types.
#![cfg_attr(docsrs, feature(doc_cfg))]

// Lets `#[facet(nbtx::...)]` resolve inside this crate's own source and tests.
extern crate self as nbtx;

// `bstr` and `facet` types are deliberately *not* re-exported. Depend on them
// directly, at the versions nbtx pins (see Cargo.toml), and enable facet's
// `bstr` feature:
//
//     facet = { git = "...", rev = "...", features = ["bstr"] }
//     bstr  = "1"
//
// A re-export would buy nothing. Any crate that derives already needs `facet`
// as a direct dependency at the same rev — `#[derive(Facet)]` expands to
// `::facet::`-rooted paths, so `nbtx::Facet` cannot stand in for it — at which
// point `facet::Facet` is in scope anyway. `BString` is a one-line dependency
// plus that feature flag, stated explicitly rather than arriving implicitly
// through nbtx's own feature selection.
//
// What re-exporting *would* cost is real: it republishes another crate's items
// as nbtx's own public API, and `facet` is pinned to a git rev of a pre-1.0
// release candidate that has already churned its internals during this project.
//
// byteorder's `BigEndian`/`LittleEndian` are the exception, and are re-exported
// below: they are used as generic parameters *in nbtx's own public signatures*
// (`to_bytes::<BigEndian>`), so they are part of this API by construction.
pub use byteorder::{BigEndian, LittleEndian};
pub use error::{Error, Result};

pub use crate::field_type::FieldType;
pub use crate::named::Named;
pub use crate::value::{Compound, Value};
pub use crate::variant::{EndiannessImpl, Variant, VarintEndian};

/// Maximum number of nested containers (`List`/`Compound`) the codecs will
/// encode or decode before giving up with [`Error::MaxDepthExceeded`].
///
/// The codecs are recursive, and a few kilobytes of nested `TAG_List` bytes
/// would otherwise drive them into a stack overflow — which in Rust aborts the
/// whole process rather than raising a catchable error. 512 is far past anything
/// a real document needs, while leaving the recursion comfortably inside a
/// default thread stack even in an unoptimised build.
pub const MAX_DEPTH: usize = 512;

/// Maximum length, in bytes, of any NBT string: a `String`-tag payload, a
/// compound key, or a root name.
///
/// `i16::MAX`, the largest length the big/little-endian variants' `u16` prefix
/// can carry without ambiguity. Always enforced on write (an unchecked `as u16`
/// would wrap the prefix and emit a stream that decodes as something else) and,
/// on read, for the varint variant, whose length prefix has no natural upper
/// bound.
pub const MAX_STRING_LEN: usize = i16::MAX as usize;

// Namespaced `#[facet(nbtx::...)]` extension attributes. `define_attr_grammar!`
// generates the `Attr` type plus the `__attr!`/`__parse_attr!` dispatcher macros
// that facet's derive expands `nbtx::<name>` into.
facet::define_attr_grammar! {
    ns "nbtx";
    crate_path ::nbtx;

    /// Container-level `#[facet(nbtx::...)]` attributes understood by nbtx.
    pub enum Attr {
        /// Silently skip compound keys that match no field of this struct.
        ///
        /// Usage: `#[facet(nbtx::allow_unknown_fields)]`
        ///
        /// Unknown keys are an error by default ([`Error::UnknownField`]) so
        /// that schema drift cannot quietly discard data; this attribute opts a
        /// single struct back into lenient decoding.
        ///
        /// Honoured by **both** codecs: the binary one (`from_*_bytes`) and the
        /// textual one (`from_string`).
        AllowUnknownFields,
    }
}

/// Builds the "nesting too deep" error. See [`MAX_DEPTH`].
///
/// Shared by every codec, so it lives here rather than in `nbt::io`: the SNBT
/// parser/writer and the [`to_value`]/[`from_value`] conversion enforce the same
/// bound as the binary ones, and the conversion is available with no features at
/// all — hence no `#[cfg]` here.
pub(crate) fn max_depth_exceeded() -> Error {
    Error::MaxDepthExceeded(crate::error::MaxDepthExceeded {
        max: MAX_DEPTH,
        #[cfg(feature = "error-context")]
        at: String::from("unknown"),
        #[cfg(feature = "error-context")]
        index: None,
    })
}

/// Errors if `depth` (the number of already-entered containers) has reached
/// [`MAX_DEPTH`]. Call this *before* recursing into a nested container.
#[inline]
pub(crate) fn check_depth(depth: usize) -> std::result::Result<(), Error> {
    if depth >= MAX_DEPTH {
        return Err(max_depth_exceeded());
    }
    Ok(())
}

/// Returns `true` if `shape` carries the `#[facet(nbtx::<key>)]` marker.
pub(crate) fn has_nbtx_attr(shape: &facet_core::Shape, key: &str) -> bool {
    shape
        .attributes
        .iter()
        .any(|a| a.ns == Some("nbtx") && a.key == key)
}

// Feature-independent: converting between a typed value and the dynamic `Value`
// tree never touches the wire format, so it is available even with no features.
pub use crate::convert::{from_value, to_value};

#[cfg(feature = "nbt")]
pub use crate::nbt::de::{from_be_bytes, from_bytes, from_le_bytes, from_varint_bytes};
#[cfg(feature = "nbt")]
pub use crate::nbt::ser::{
    Serializer, to_be_bytes, to_be_bytes_in, to_bytes, to_bytes_in, to_le_bytes, to_le_bytes_in,
    to_varint_bytes, to_varint_bytes_in,
};

#[cfg(feature = "snbt")]
pub use snbt::{from_string, to_string};

mod convert;
mod error;
mod field_type;
mod named;
mod reflect;
mod value;
mod variant;

#[cfg(feature = "nbt")]
mod nbt;

#[cfg(feature = "snbt")]
pub mod snbt;
