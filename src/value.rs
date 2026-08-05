use std::hash::{Hash, Hasher};

use bstr::BString;
use facet::Facet;

/// The map type backing [`Value::Compound`].
///
/// An order-preserving [`indexmap::IndexMap`] with the default `preserve_order`
/// feature (so on-disk key order round-trips), or a sorted
/// [`BTreeMap`](std::collections::BTreeMap) with `--no-default-features`.
#[cfg(feature = "preserve_order")]
pub type Compound = indexmap::IndexMap<BString, Value>;
/// The map type backing [`Value::Compound`] (sorted `BTreeMap` variant; enable
/// the `preserve_order` feature for order preservation).
#[cfg(not(feature = "preserve_order"))]
pub type Compound = std::collections::BTreeMap<BString, Value>;

/// General NBT value type that can represent any value.
///
/// In case the structure of some piece of NBT data is not known, this
/// type can be used to deserialise it. Unlike a `#[derive(Facet)]` struct,
/// a [`Value`] preserves the exact NBT tag of every node (so `ByteArray`,
/// `IntArray`, `LongArray` and `List` stay distinct) and stores strings as
/// [`bstr::BString`], so non-UTF-8 payloads round-trip losslessly.
///
/// # Equality
///
/// `Value` is `Eq + Hash`, so a whole document can key a `HashMap`/`HashSet`.
/// Its float payloads use a *total* equality — IEEE-754's, plus every `NaN`
/// equal to every other `NaN` — because `Eq` needs a reflexive `==` and
/// `f32`/`f64` do not have one. `-0.0 == 0.0` still holds, and `Hash`
/// normalises both cases so equal values always hash alike. Comparing `NaN`
/// *payload bits* therefore needs `to_bits()`, not `==`.
///
/// # Facet representation
///
/// `Value` derives [`Facet`] so that it can be used as a type argument to the
/// public (de)serialisation functions (e.g. `to_be_bytes::<Value>`). Its
/// [`bstr::BString`] and [`Compound`] map payloads reflect directly (facet's
/// `bstr`/`indexmap` support), so no proxy types are needed. The NBT and SNBT
/// codecs still detect a `Value` by its shape id and read/write the real Rust
/// value directly, preserving full `BString` fidelity and key order.
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum Value {
    /// A signed byte.
    Byte(i8),
    /// A signed short.
    Short(i16),
    /// A signed int.
    Int(i32),
    /// A signed long.
    Long(i64),
    /// A signed float
    Float(f32),
    /// A signed double
    Double(f64),
    /// A byte array.
    ///
    /// Deserialising an NBT `ByteArray` tag into a [`Value`] yields this
    /// variant, and it re-serialises byte-for-byte as a `ByteArray` tag for all
    /// three endianness variants. In a `#[derive(Facet)]` struct, a `Vec<u8>`
    /// field maps to the `ByteArray` tag; any other `Vec<T>` maps to a `List`.
    ByteArray(Vec<u8>),
    /// A string of raw bytes.
    ///
    /// NBT strings are not guaranteed to be valid UTF-8 (Bedrock allows
    /// arbitrary bytes in a string tag), so the raw bytes are stored in a
    /// [`bstr::BString`] rather than a [`String`]. Construction from string
    /// literals still works ergonomically, e.g. `Value::String("abc".into())`.
    String(BString),
    /// List of an arbitrary NBT value.
    List(Vec<Value>),
    /// Key-value map.
    ///
    /// Keys are stored as [`bstr::BString`] because NBT string keys are not
    /// guaranteed to be valid UTF-8. The backing map is a [`Compound`] (an
    /// order-preserving `IndexMap` by default; see the `preserve_order` feature).
    Compound(Compound),
    /// An array of integers.
    ///
    /// Tag 11. The highest tag universally supported: NBT readers written
    /// against the original tag set stop here.
    IntArray(Vec<i32>),
    /// An array of longs.
    ///
    /// Tag 12. Valid Bedrock NBT, but it postdates the original tag set, so
    /// older or Java-Edition-shaped readers cap at `IntArray` (11) and reject it
    /// as an unknown tag type. Producing a `LongArray` is therefore fine for
    /// Bedrock, but will not round-trip through every NBT tool.
    LongArray(Vec<i64>),
}

impl Value {
    /// Returns the NBT tag discriminant (1-12) for this value.
    #[inline]
    #[must_use]
    pub fn discriminant(&self) -> u8 {
        match self {
            Self::Byte(_) => 1,
            Self::Short(_) => 2,
            Self::Int(_) => 3,
            Self::Long(_) => 4,
            Self::Float(_) => 5,
            Self::Double(_) => 6,
            Self::ByteArray(_) => 7,
            Self::String(_) => 8,
            Self::List(_) => 9,
            Self::Compound(_) => 10,
            Self::IntArray(_) => 11,
            Self::LongArray(_) => 12,
        }
    }
}

macro_rules! impl_access_fns {
    ($($tag: ident = $ty: ty),+) => {
        $(paste::paste! {
            #[inline]
            #[doc = concat!(
                "Returns the inner value if the tag is of a, ", stringify!($tag), ", otherwise returns self.
                This method is the same as [`as_", stringify!([<$tag:snake>]), "`](Self::as_", stringify!([<$tag:snake>]), ") but instead takes ownership of the value."
            )]
            pub fn [<into_ $tag:snake>](self) -> Result<$ty, Self> {
                match self {
                    Self::$tag(val) => Ok(val),
                    _ => Err(self)
                }
            }

            #[inline]
            #[doc = concat!(
                "Returns a reference to the inner value of the tag is the requested type is present.
                Use [`into_", stringify!([<$tag:snake>]), "`](Self::into_", stringify!([<$tag:snake>]), ")."
            )]
            pub fn [<as_ $tag:snake>](&self) -> Option<&$ty> {
                match self {
                    Self::$tag(val) => Some(val),
                    _ => None
                }
            }

            #[inline]
            #[doc = concat!(
                "Returns whether the inner value is of type `", stringify!($tag), "`."
            )]
            pub fn [<is_ $tag:snake>](&self) -> bool {
                matches!(self, Self::$tag(_))
            }
        })+
    }
}

impl Value {
    impl_access_fns!(
        Byte = i8,
        Short = i16,
        Int = i32,
        Long = i64,
        Float = f32,
        Double = f64,
        String = BString,
        List = Vec<Self>,
        Compound = Compound,
        ByteArray = Vec<u8>,
        IntArray = Vec<i32>,
        LongArray = Vec<i64>
    );
}

impl From<BString> for Value {
    #[inline]
    fn from(value: BString) -> Self {
        Value::String(value)
    }
}

impl From<String> for Value {
    #[inline]
    fn from(value: String) -> Self {
        Value::String(BString::from(value))
    }
}

impl From<&str> for Value {
    #[inline]
    fn from(value: &str) -> Self {
        Value::String(BString::from(value))
    }
}

/// Total equality for the float payloads: IEEE-754 equality, plus every `NaN`
/// equal to every other `NaN`.
///
/// This is what makes [`Value`] `Eq` (and so usable as a `HashMap`/`HashSet`
/// key) despite holding `f32`/`f64`, whose own `==` is not reflexive. It is a
/// genuine equivalence relation: reflexive because a `NaN` now equals itself,
/// symmetric by construction, and transitive because the `NaN`s form one class
/// and the ordinary IEEE classes are untouched.
///
/// `-0.0 == 0.0` is *kept* — that is IEEE's answer, it is what this crate's
/// `PartialEq` has always said, and [`Value`]'s [`Hash`] already normalises the
/// two to the same bytes so that a key stored as `-0.0` is still found by
/// `0.0`. `Hash` normalises `NaN` payloads the same way, so the two impls agree
/// on every input.
#[inline]
fn float_eq<T: Copy + PartialEq + Into<f64>>(lhs: T, rhs: T) -> bool {
    let (l, r) = (lhs.into(), rhs.into());
    l == r || (l.is_nan() && r.is_nan())
}

impl PartialEq<Value> for Value {
    #[inline]
    fn eq(&self, rhs: &Value) -> bool {
        match self {
            Value::Byte(lhs) => rhs.as_byte() == Some(lhs),
            Value::Short(lhs) => rhs.as_short() == Some(lhs),
            Value::Int(lhs) => rhs.as_int() == Some(lhs),
            Value::Long(lhs) => rhs.as_long() == Some(lhs),
            Value::Float(lhs) => rhs.as_float().is_some_and(|rhs| float_eq(*lhs, *rhs)),
            Value::Double(lhs) => rhs.as_double().is_some_and(|rhs| float_eq(*lhs, *rhs)),
            Value::ByteArray(lhs) => rhs.as_byte_array().is_some_and(|rhs| lhs.as_slice() == rhs),
            Value::String(lhs) => rhs.as_string() == Some(lhs),
            Value::List(lhs) => rhs.as_list() == Some(lhs),
            Value::Compound(lhs) => rhs.as_compound() == Some(lhs),
            Value::IntArray(lhs) => rhs.as_int_array() == Some(lhs),
            Value::LongArray(lhs) => rhs.as_long_array() == Some(lhs),
        }
    }
}

macro_rules! impl_scalar_eq {
    ($($ty: ty => $as: ident),+) => {
        $(
            impl PartialEq<$ty> for Value {
                #[inline]
                fn eq(&self, rhs: &$ty) -> bool {
                    self.$as() == Some(rhs)
                }
            }
            impl PartialEq<$ty> for &Value {
                #[inline]
                fn eq(&self, rhs: &$ty) -> bool {
                    self.$as() == Some(rhs)
                }
            }
            impl PartialEq<$ty> for &mut Value {
                #[inline]
                fn eq(&self, rhs: &$ty) -> bool {
                    self.$as() == Some(rhs)
                }
            }
        )+
    }
}

impl_scalar_eq!(
    i8 => as_byte,
    i16 => as_short,
    i32 => as_int,
    i64 => as_long
);

/// The float comparisons use the same total equality as `Value == Value` (see
/// [`float_eq`]), so `Value::Double(f64::NAN) == f64::NAN` holds. Comparing a
/// `Value` follows `Value`'s equality model whichever side the number is on;
/// having `v == other_value` and `v == raw_float` disagree about `NaN` would be
/// the more surprising rule.
macro_rules! impl_float_eq {
    ($($ty: ty => $as: ident),+) => {
        $(
            impl PartialEq<$ty> for Value {
                #[inline]
                fn eq(&self, rhs: &$ty) -> bool {
                    self.$as().is_some_and(|lhs| float_eq(*lhs, *rhs))
                }
            }
            impl PartialEq<$ty> for &Value {
                #[inline]
                fn eq(&self, rhs: &$ty) -> bool {
                    self.$as().is_some_and(|lhs| float_eq(*lhs, *rhs))
                }
            }
            impl PartialEq<$ty> for &mut Value {
                #[inline]
                fn eq(&self, rhs: &$ty) -> bool {
                    self.$as().is_some_and(|lhs| float_eq(*lhs, *rhs))
                }
            }
        )+
    }
}

impl_float_eq!(
    f32 => as_float,
    f64 => as_double
);

macro_rules! impl_slice_eq {
    ($($ty: ty => $as: ident),+) => {
        $(
            impl PartialEq<$ty> for Value {
                #[inline]
                fn eq(&self, rhs: &$ty) -> bool {
                    self.$as().is_some_and(|lhs| lhs == rhs)
                }
            }
            impl PartialEq<$ty> for &Value {
                #[inline]
                fn eq(&self, rhs: &$ty) -> bool {
                    self.$as().is_some_and(|lhs| lhs == rhs)
                }
            }
            impl PartialEq<$ty> for &mut Value {
                #[inline]
                fn eq(&self, rhs: &$ty) -> bool {
                    self.$as().is_some_and(|lhs| lhs == rhs)
                }
            }
        )+
    }
}

impl_slice_eq!(
    &[u8] => as_byte_array,
    &[Value] => as_list,
    &[i32] => as_int_array,
    &[i64] => as_long_array
);

impl PartialEq<&str> for Value {
    #[inline]
    fn eq(&self, rhs: &&str) -> bool {
        self.as_string()
            .is_some_and(|lhs| lhs.as_slice() == rhs.as_bytes())
    }
}

impl PartialEq<&str> for &Value {
    #[inline]
    fn eq(&self, rhs: &&str) -> bool {
        self.as_string()
            .is_some_and(|lhs| lhs.as_slice() == rhs.as_bytes())
    }
}

impl PartialEq<&str> for &mut Value {
    #[inline]
    fn eq(&self, rhs: &&str) -> bool {
        self.as_string()
            .is_some_and(|lhs| lhs.as_slice() == rhs.as_bytes())
    }
}

impl PartialEq<Compound> for Value {
    #[inline]
    fn eq(&self, rhs: &Compound) -> bool {
        self.as_compound() == Some(rhs)
    }
}

impl PartialEq<Compound> for &Value {
    #[inline]
    fn eq(&self, rhs: &Compound) -> bool {
        self.as_compound() == Some(rhs)
    }
}

impl PartialEq<Compound> for &mut Value {
    #[inline]
    fn eq(&self, rhs: &Compound) -> bool {
        self.as_compound() == Some(rhs)
    }
}

/// `Value`'s equality is total (see the `float_eq` helper), so it is `Eq` as well as
/// `PartialEq` and can key a `HashMap`/`HashSet` or sit in a `HashSet`-backed
/// set of documents. The two float rules that make this sound are that every
/// `NaN` equals every other `NaN` and that `-0.0 == 0.0`; [`Hash`] normalises
/// both cases to the same bytes, so equal values always hash alike.
impl Eq for Value {}

impl Hash for Value {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        match self {
            Value::Byte(v) => state.write_i8(*v),
            Value::Short(v) => state.write_i16(*v),
            Value::Int(v) => state.write_i32(*v),
            Value::Long(v) => state.write_i64(*v),
            Value::String(v) => state.write(v.as_slice()),
            // `f32`/`f64` are not `Hash`, so hash their bytes — but only after
            // collapsing the two cases where equal floats have different bit
            // patterns, or a `Value` used as a map key would go missing:
            // `-0.0 == 0.0`, and every `NaN` equals every other `NaN` under
            // `float_eq`, whatever payload it carries.
            Value::Float(v) => {
                let normalized = if v.is_nan() {
                    f32::NAN
                } else if *v == 0_f32 {
                    0_f32
                } else {
                    *v
                };
                state.write(&normalized.to_le_bytes());
            }
            Value::Double(v) => {
                // See `Value::Float` above.
                let normalized = if v.is_nan() {
                    f64::NAN
                } else if *v == 0_f64 {
                    0_f64
                } else {
                    *v
                };
                state.write(&normalized.to_le_bytes());
            }
            Value::Compound(map) => {
                for (k, v) in map {
                    state.write(k.as_slice());
                    v.hash(state);
                }
            }
            Value::List(v) => Self::hash_slice(v, state),
            Value::ByteArray(v) => u8::hash_slice(v, state),
            Value::IntArray(v) => i32::hash_slice(v, state),
            Value::LongArray(v) => i64::hash_slice(v, state),
        }
    }
}
