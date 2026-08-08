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

    /// The `#[facet(nbtx::...)]` extension attributes nbtx understands.
    ///
    /// [`AllowUnknownFields`](Attr::AllowUnknownFields) and
    /// [`VariantAs`](Attr::VariantAs) go on a container (a struct and an enum
    /// respectively); [`LenientWidth`](Attr::LenientWidth) goes on a field, or
    /// on an enum beside its `variant_as`.
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

        /// **Mandatory on every enum**: how this enum's variants are written.
        ///
        /// Usage: `#[facet(nbtx::variant_as(<mode>))]`, where `<mode>` is one of
        /// the bare type names `u8`, `i8`, `u16`, `i16`, `u32`, `i32`, `u64`,
        /// `i64` or `str`.
        ///
        /// ```
        /// use facet::Facet;
        ///
        /// #[derive(Facet)]
        /// #[facet(nbtx::variant_as(str))]
        /// #[repr(u8)]
        /// enum Mode {
        ///     Survival,
        ///     #[facet(rename = "creative")]
        ///     Creative,
        /// }
        ///
        /// #[derive(Facet)]
        /// #[facet(nbtx::variant_as(u8))]
        /// #[repr(u8)]
        /// enum Difficulty {
        ///     Peaceful = 0,
        ///     Hard = 3,
        /// }
        /// ```
        ///
        /// * `str` writes the variant's name as a `String` tag, honouring
        ///   `#[facet(rename = "...")]` on the variant.
        /// * The eight integer modes write the *active variant's discriminant*
        ///   as a fixed-width NBT scalar: `u8`/`i8` → `Byte`, `u16`/`i16` →
        ///   `Short`, `u32`/`i32` → `Int`, `u64`/`i64` → `Long`. Number the
        ///   variants with plain Rust discriminants (`Hard = 3`) to pin the
        ///   values a document uses.
        ///
        /// The mode is a *wire* choice and is independent of the `#[repr(...)]`
        /// that gives the enum its Rust layout (facet's derive requires one of
        /// those regardless). `usize`/`isize` are deliberately not accepted:
        /// their width varies by target.
        ///
        /// There is no default. An enum without this attribute is an
        /// [`Error::MissingVariantAs`] from every codec — the binary one
        /// (`to_*_bytes`/`from_*_bytes`), the textual one
        /// (`to_string`/`from_string`) and [`to_value`]/[`from_value`] alike.
        /// The wire form of an enum is part of a document's schema, and a silent
        /// default would let it change under a Rust-side edit.
        VariantAs(shape_type),

        /// **Decode only**: extra NBT scalar tags this field will accept, on
        /// top of its own.
        ///
        /// Usage: `#[facet(nbtx::lenient_width(<types>))]`, where `<types>` is
        /// one or more of the bare type names `i8`, `i16`, `i32`, `i64`, `f32`
        /// or `f64` — the six native NBT scalar wire types (`Byte`, `Short`,
        /// `Int`, `Long`, `Float`, `Double`).
        ///
        /// Real documents drift: the same key is written as a `Byte` by one
        /// version of a game and as an `Int` by the next, and a decoder that
        /// insists on exactly one tag cannot read both. This attribute names the
        /// *other* tags a field tolerates, and nbtx converts each of them into
        /// the field's declared Rust type — **losslessly**, or not at all.
        ///
        /// ```
        /// use facet::Facet;
        ///
        /// #[derive(Facet, Debug, PartialEq)]
        /// struct ItemRotation {
        ///     // Written as a `Float` today, as a `Byte` by older writers.
        ///     #[facet(nbtx::lenient_width(i8))]
        ///     rotation: f32,
        /// }
        ///
        /// // A `Byte` tag arriving where a `Float` was declared.
        /// let value = nbtx::Value::Compound(nbtx::Compound::from_iter([
        ///     ("rotation".into(), nbtx::Value::Byte(2)),
        /// ]));
        /// let item: ItemRotation = nbtx::from_value(value)?;
        /// assert_eq!(item, ItemRotation { rotation: 2.0 });
        /// # Ok::<(), nbtx::Error>(())
        /// ```
        ///
        /// # Where it may be written
        ///
        /// * On a field whose type is one of the six scalars.
        /// * On an `Option<T>`, `Vec<T>` or `[T; N]` field of one of them: the
        ///   single declaration widens *every* element uniformly.
        /// * On an **enum**, next to its mandatory
        ///   [`variant_as`](Attr::VariantAs), where it widens the tag the
        ///   *discriminant* may arrive in:
        ///
        ///   ```
        ///   use facet::Facet;
        ///
        ///   #[derive(Facet)]
        ///   #[facet(nbtx::variant_as(i32), nbtx::lenient_width(i8))]
        ///   #[repr(u8)]
        ///   enum Difficulty {
        ///       Peaceful = 0,
        ///       Hard = 3,
        ///   }
        ///   ```
        ///
        ///   The discriminant is normally an `Int`; a `Byte` is now accepted
        ///   too, range-checked exactly as a lenient scalar field would be. It
        ///   is rejected on a `variant_as(str)` enum, whose variants are not
        ///   numbers at all.
        ///
        ///   For an **unsigned** mode (`u8`/`u16`/`u32`) the widened value must
        ///   first fit the mode's own tag width as a *signed* integer, bit for
        ///   bit — the same convention the non-lenient case already uses (see
        ///   [`VariantAs`](crate::reflect::VariantAs)). So for a `u8` mode, a
        ///   lenient `Int(200)` does not resolve to discriminant 200 (200 does
        ///   not fit `i8`) but a lenient `Int(-56)` does — -56 is the bit
        ///   pattern of unsigned 200 in a byte. This mirrors, rather than
        ///   loosens, how the discriminant is read at its natural width.
        ///
        /// Anywhere else — a struct-typed field, a `Vec<Struct>`, an
        /// `Option<Struct>`, a `bool`, a `u8`, a `String`, a [`Value`], a map —
        /// is an [`Error::InvalidLenientWidth`], raised the first time that
        /// field is decoded. Widening has no meaning for a compound, so it is
        /// refused rather than silently ignored.
        ///
        /// # What conversions are allowed
        ///
        /// The field's own tag is always accepted, listed or not. A *different*
        /// tag that the list does not name stays an
        /// [`Error::UnexpectedType`], unchanged. A tag the list *does* name is
        /// converted, and only a conversion that loses nothing succeeds:
        ///
        /// * integer → wider integer: always.
        /// * integer → narrower integer: only if the value is in the target's
        ///   range.
        /// * integer → float: only if the float reproduces the integer exactly.
        /// * float → integer: only if the float has no fractional part and fits.
        /// * float → float: only if the value survives the target's precision.
        ///
        /// Anything else is an [`Error::LenientWidthOutOfRange`] — a distinct
        /// failure from "wrong tag entirely", because the schema *did* allow
        /// that tag and it is this particular value that will not fit.
        ///
        /// # A footgun in SNBT: `f32` and `f64` are not interchangeable here
        ///
        /// In the textual codec a literal's tag comes from its own suffix —
        /// `3.0` is a `Double`, `3.0f` a `Float` — *before* this attribute is
        /// ever consulted. Naming only `f32` therefore does **not** widen a
        /// bare, suffixless decimal literal into an integer field: `3.0` is
        /// already typed `Double`, so only `lenient_width(f64)` reaches it;
        /// `lenient_width(f32)` only reaches the explicitly `f32`-suffixed
        /// spelling `3.0f`. To accept either spelling of a decimal literal,
        /// name **both** `f32` and `f64`. This asymmetry does not exist in the
        /// binary codec, where the wire tag alone decides.
        ///
        /// # Decode only
        ///
        /// Encoding ignores this attribute completely: a field is always written
        /// with its own declared tag, never with one from the list. That makes
        /// the attribute a one-way *normaliser* — read the old shapes, write the
        /// current one — not a round-trip-preserving tolerance. Re-encoding a
        /// document that was decoded leniently therefore changes those tags on
        /// purpose.
        ///
        /// Honoured by every decoder: the binary one (`from_*_bytes`), the
        /// textual one (`from_string`) and [`from_value`].
        LenientWidth(list(shape_type)),
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
        .any(|a| a.ns() == Some("nbtx") && a.key() == key)
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
