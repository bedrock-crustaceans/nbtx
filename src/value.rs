use std::hash::{Hash, Hasher};

use bstr::BString;
use facet::Facet;

use crate::reflect::value_tag;
use crate::{Error, FieldType};

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
    /// A list of values that all share one NBT tag.
    ///
    /// The payload is a [`ValueList`], not a `Vec<Value>`: the wire format
    /// stores a single element-type byte for the whole list, so the elements
    /// cannot differ in tag, and an *empty* list still has to remember the tag
    /// it would have held. Build one with
    /// [`ValueList::try_from`](ValueList::try_from) from a `Vec<Value>`, or
    /// name the variant directly (`ValueList::Byte(vec![1, 2])`).
    List(ValueList),
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
        List = ValueList,
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

/// Hashes an `f32` the way [`float_eq`] compares it.
///
/// `f32` is not `Hash`, so its bytes are hashed — but only after collapsing the
/// two cases where *equal* floats have different bit patterns, or a value used
/// as a map key would go missing: `-0.0 == 0.0`, and every `NaN` equals every
/// other `NaN` whatever payload it carries.
#[inline]
fn hash_f32<H: Hasher>(v: f32, state: &mut H) {
    let normalized = if v.is_nan() {
        f32::NAN
    } else if v == 0_f32 {
        0_f32
    } else {
        v
    };
    state.write(&normalized.to_le_bytes());
}

/// Hashes an `f64` the way [`float_eq`] compares it. See [`hash_f32`].
#[inline]
fn hash_f64<H: Hasher>(v: f64, state: &mut H) {
    let normalized = if v.is_nan() {
        f64::NAN
    } else if v == 0_f64 {
        0_f64
    } else {
        v
    };
    state.write(&normalized.to_le_bytes());
}

/// Hashes a byte string **length first**.
///
/// Every variable-length payload goes through here (or writes its own length
/// the same way) so that the bytes of one element can never be mistaken for the
/// bytes of two: without the separator, `ByteArray([[1, 2], []])` and
/// `ByteArray([[1], [2]])` — which are not equal — feed the hasher the identical
/// stream. `Hash for [T]` length-prefixes for exactly this reason; `Hasher::write`
/// on a raw slice does not.
#[inline]
fn hash_bytes<H: Hasher>(bytes: &[u8], state: &mut H) {
    state.write_usize(bytes.len());
    state.write(bytes);
}

/// Hashes a fixed-width slice length first. See [`hash_bytes`]: the elements
/// themselves are self-delimiting, but a *sequence* of them is not.
#[inline]
fn hash_len_prefixed<T: Hash, H: Hasher>(items: &[T], state: &mut H) {
    state.write_usize(items.len());
    T::hash_slice(items, state);
}

/// Hashes a compound: entry count, then each key length-first and its value.
/// See [`hash_bytes`] for why the lengths are there.
fn hash_compound<H: Hasher>(map: &Compound, state: &mut H) {
    state.write_usize(map.len());
    for (k, v) in map {
        hash_bytes(k.as_slice(), state);
        v.hash(state);
    }
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

// No `&[Value] => as_list` arm: a list's payload is a [`ValueList`], which is
// not a slice of `Value` at all. Compare against a `ValueList` instead (see the
// impls below), or against `list.to_values()`.
impl_slice_eq!(
    &[u8] => as_byte_array,
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
            Value::String(v) => hash_bytes(v.as_slice(), state),
            // `f32`/`f64` are not `Hash`, so hash their bytes — but only after
            // collapsing the two cases where equal floats have different bit
            // patterns, or a `Value` used as a map key would go missing:
            // `-0.0 == 0.0`, and every `NaN` equals every other `NaN` under
            // `float_eq`, whatever payload it carries.
            Value::Float(v) => hash_f32(*v, state),
            Value::Double(v) => hash_f64(*v, state),
            Value::Compound(map) => hash_compound(map, state),
            Value::List(v) => v.hash(state),
            // Length first; see `hash_bytes`.
            Value::ByteArray(v) => hash_bytes(v, state),
            Value::IntArray(v) => hash_len_prefixed(v, state),
            Value::LongArray(v) => hash_len_prefixed(v, state),
        }
    }
}

/// The typed payload of a [`Value::List`]: one variant per NBT element type.
///
/// A `TAG_List` on the wire is a *single* element-type byte followed by N
/// payloads of that one type. A `Vec<Value>` cannot express that faithfully: it
/// admits mixtures which have no encoding at all, and it forgets the element
/// type the moment the list is empty (so an empty `List<Byte>` would come back
/// out as a `List<End>`). Both problems disappear when the payload is a typed
/// enum — a `ValueList` is homogeneous by construction, and an empty one still
/// remembers what it would have held.
///
/// # `End` is not the same as "empty"
///
/// [`ValueList::End`] is the list whose element-type byte is `TAG_End`. That is
/// the *only* legal use of `TAG_End` as an element type, and only at length 0
/// (a non-empty `End` list is a decode error, as it is in pmmp/NBT and
/// gophertunnel). It is deliberately **not** equal to
/// `ValueList::Int(Vec::new())`: those two write different bytes — element type
/// 0 versus 3 — so treating them as one value would make the round-trip lossy.
/// SNBT cannot tell them apart (`[]` is all the syntax there is), which is why
/// `[]` always parses back as `End`.
///
/// # Equality
///
/// `Eq + Hash`, with the same *total* float equality as [`Value`]: every `NaN`
/// equals every other `NaN`, `-0.0 == 0.0`, and [`Hash`] normalises both so
/// equal lists always hash alike.
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum ValueList {
    /// The empty list, with element type `TAG_End` (tag byte 0, length 0).
    ///
    /// Also what an unconstrained empty list is: [`ValueList::default`] and
    /// `ValueList::try_from(Vec::new())` both land here, and [`Self::push`]
    /// turns it into a typed list on first use.
    End,
    /// A list of `Byte` tags.
    Byte(Vec<i8>),
    /// A list of `Short` tags.
    Short(Vec<i16>),
    /// A list of `Int` tags.
    Int(Vec<i32>),
    /// A list of `Long` tags.
    Long(Vec<i64>),
    /// A list of `Float` tags.
    Float(Vec<f32>),
    /// A list of `Double` tags.
    Double(Vec<f64>),
    /// A list of `ByteArray` tags.
    ByteArray(Vec<Vec<u8>>),
    /// A list of `String` tags.
    ///
    /// [`bstr::BString`], never [`String`]: NBT strings are raw bytes and are
    /// not guaranteed to be valid UTF-8, exactly as in [`Value::String`].
    String(Vec<BString>),
    /// A list of `List` tags.
    ///
    /// The *inner* lists need not agree with each other: `[[1b],["x"],[]]` is a
    /// list of three lists whose element types are `Byte`, `String` and `End`.
    /// Only the outer element type — `List` — is shared.
    List(Vec<ValueList>),
    /// A list of `Compound` tags.
    Compound(Vec<Compound>),
    /// A list of `IntArray` tags.
    IntArray(Vec<Vec<i32>>),
    /// A list of `LongArray` tags.
    LongArray(Vec<Vec<i64>>),
}

impl ValueList {
    /// The NBT tag every element of this list carries — the element-type byte
    /// the binary encoding writes.
    ///
    /// [`FieldType::End`] for [`ValueList::End`], which is the empty list that
    /// names no element type.
    #[inline]
    #[must_use]
    pub fn element_type(&self) -> FieldType {
        match self {
            Self::End => FieldType::End,
            Self::Byte(_) => FieldType::Byte,
            Self::Short(_) => FieldType::Short,
            Self::Int(_) => FieldType::Int,
            Self::Long(_) => FieldType::Long,
            Self::Float(_) => FieldType::Float,
            Self::Double(_) => FieldType::Double,
            Self::ByteArray(_) => FieldType::ByteArray,
            Self::String(_) => FieldType::String,
            Self::List(_) => FieldType::List,
            Self::Compound(_) => FieldType::Compound,
            Self::IntArray(_) => FieldType::IntArray,
            Self::LongArray(_) => FieldType::LongArray,
        }
    }

    /// The number of elements in the list.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::End => 0,
            Self::Byte(v) => v.len(),
            Self::Short(v) => v.len(),
            Self::Int(v) => v.len(),
            Self::Long(v) => v.len(),
            Self::Float(v) => v.len(),
            Self::Double(v) => v.len(),
            Self::ByteArray(v) => v.len(),
            Self::String(v) => v.len(),
            Self::List(v) => v.len(),
            Self::Compound(v) => v.len(),
            Self::IntArray(v) => v.len(),
            Self::LongArray(v) => v.len(),
        }
    }

    /// Whether the list has no elements.
    ///
    /// True for [`ValueList::End`] and for every typed list of length 0; use
    /// [`Self::element_type`] to tell those apart.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether this is the untyped empty list, [`ValueList::End`].
    #[inline]
    #[must_use]
    pub fn is_end(&self) -> bool {
        matches!(self, Self::End)
    }

    /// The empty list of element type `element_type`.
    ///
    /// The variant an empty list has to be built at is exactly what a
    /// `Vec<T>`-shaped Rust value cannot say for itself, so the codecs need a
    /// way to say it: encoding an empty `Vec<String>` writes element type
    /// `String`, and so must converting one to a [`Value`].
    /// [`FieldType::End`] gives [`ValueList::End`].
    #[must_use]
    pub fn empty(element_type: FieldType) -> Self {
        match element_type {
            FieldType::End => Self::End,
            FieldType::Byte => Self::Byte(Vec::new()),
            FieldType::Short => Self::Short(Vec::new()),
            FieldType::Int => Self::Int(Vec::new()),
            FieldType::Long => Self::Long(Vec::new()),
            FieldType::Float => Self::Float(Vec::new()),
            FieldType::Double => Self::Double(Vec::new()),
            FieldType::ByteArray => Self::ByteArray(Vec::new()),
            FieldType::String => Self::String(Vec::new()),
            FieldType::List => Self::List(Vec::new()),
            FieldType::Compound => Self::Compound(Vec::new()),
            FieldType::IntArray => Self::IntArray(Vec::new()),
            FieldType::LongArray => Self::LongArray(Vec::new()),
        }
    }

    /// Appends `value`, or reports [`Error::HeterogeneousList`] if its tag is
    /// not this list's element type.
    ///
    /// An [`End`](ValueList::End) list has no element type yet, so it *adopts*
    /// the first pushed element's — the same rule pmmp/NBT's `ListTag::push`
    /// applies. Once adopted the type is fixed: the list can only be widened
    /// again by replacing it.
    ///
    /// # Errors
    ///
    /// [`Error::HeterogeneousList`], naming this list's element type as
    /// `expected` and `value`'s tag as `found`.
    pub fn push(&mut self, value: Value) -> Result<(), Error> {
        if self.is_end() {
            // A `Value` always carries a real tag, so this is never `End`
            // again: the list comes out of here typed.
            *self = Self::empty(value_tag(&value));
        }
        match (&mut *self, value) {
            (Self::Byte(items), Value::Byte(v)) => items.push(v),
            (Self::Short(items), Value::Short(v)) => items.push(v),
            (Self::Int(items), Value::Int(v)) => items.push(v),
            (Self::Long(items), Value::Long(v)) => items.push(v),
            (Self::Float(items), Value::Float(v)) => items.push(v),
            (Self::Double(items), Value::Double(v)) => items.push(v),
            (Self::ByteArray(items), Value::ByteArray(v)) => items.push(v),
            (Self::String(items), Value::String(v)) => items.push(v),
            (Self::List(items), Value::List(v)) => items.push(v),
            (Self::Compound(items), Value::Compound(v)) => items.push(v),
            (Self::IntArray(items), Value::IntArray(v)) => items.push(v),
            (Self::LongArray(items), Value::LongArray(v)) => items.push(v),
            (this, other) => {
                return Err(Error::HeterogeneousList {
                    expected: this.element_type(),
                    found: value_tag(&other),
                });
            }
        }
        Ok(())
    }

    /// The element at `index`, **cloned** into a [`Value`], or `None` if the
    /// index is out of bounds.
    ///
    /// The elements are stored unboxed (a `Vec<i8>`, not a `Vec<Value>`), so
    /// there is no `Value` to borrow and one has to be built; for a `Compound`
    /// or a nested `List` element that is a deep copy. Match on the variant
    /// directly to read an element without copying it.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<Value> {
        Some(match self {
            Self::End => return None,
            Self::Byte(v) => Value::Byte(*v.get(index)?),
            Self::Short(v) => Value::Short(*v.get(index)?),
            Self::Int(v) => Value::Int(*v.get(index)?),
            Self::Long(v) => Value::Long(*v.get(index)?),
            Self::Float(v) => Value::Float(*v.get(index)?),
            Self::Double(v) => Value::Double(*v.get(index)?),
            Self::ByteArray(v) => Value::ByteArray(v.get(index)?.clone()),
            Self::String(v) => Value::String(v.get(index)?.clone()),
            Self::List(v) => Value::List(v.get(index)?.clone()),
            Self::Compound(v) => Value::Compound(v.get(index)?.clone()),
            Self::IntArray(v) => Value::IntArray(v.get(index)?.clone()),
            Self::LongArray(v) => Value::LongArray(v.get(index)?.clone()),
        })
    }

    /// Iterates the elements as owned [`Value`]s.
    ///
    /// Every item is **cloned** out of the typed storage; see [`Self::get`].
    pub fn iter(&self) -> impl ExactSizeIterator<Item = Value> + '_ {
        // `index < self.len()` on every step, and each variant's storage has
        // exactly that many elements, so `get` never answers `None` here.
        (0..self.len()).map(move |i| self.get(i).unwrap_or(Value::Byte(0)))
    }

    /// Collects the elements into a `Vec<Value>`, cloning each. See
    /// [`Self::get`].
    #[must_use]
    pub fn to_values(&self) -> Vec<Value> {
        self.iter().collect()
    }

    /// Consumes the list into a `Vec<Value>`, boxing each element into its tag.
    ///
    /// The inverse of [`TryFrom<Vec<Value>>`](ValueList::try_from), and lossy in
    /// exactly one way: the element type of an *empty* list is not recoverable
    /// from the empty `Vec` it produces.
    #[must_use]
    pub fn into_values(self) -> Vec<Value> {
        self.into_iter().collect()
    }
}

impl ValueList {
    impl_access_fns!(
        Byte = Vec<i8>,
        Short = Vec<i16>,
        Int = Vec<i32>,
        Long = Vec<i64>,
        Float = Vec<f32>,
        Double = Vec<f64>,
        String = Vec<BString>,
        List = Vec<Self>,
        Compound = Vec<Compound>,
        ByteArray = Vec<Vec<u8>>,
        IntArray = Vec<Vec<i32>>,
        LongArray = Vec<Vec<i64>>
    );
}

/// The untyped empty list — the list an empty `Vec<Value>` converts to, and the
/// one [`ValueList::push`] gives a type to.
impl Default for ValueList {
    #[inline]
    fn default() -> Self {
        Self::End
    }
}

impl TryFrom<Vec<Value>> for ValueList {
    type Error = Error;

    /// Collects a `Vec<Value>` into the typed list its elements describe.
    ///
    /// This is the ergonomic way to build a `ValueList` from dynamic values,
    /// and the point at which a mixture is caught: NBT has no encoding for one.
    /// An empty `Vec` becomes [`ValueList::End`], the empty list that names no
    /// element type.
    ///
    /// # Errors
    ///
    /// [`Error::HeterogeneousList`] if the elements do not all share the first
    /// one's tag; `expected` is that first tag and `found` the first that
    /// differed.
    fn try_from(values: Vec<Value>) -> Result<Self, Error> {
        let mut out = ValueList::End;
        for value in values {
            out.push(value)?;
        }
        Ok(out)
    }
}

/// Iterates a [`ValueList`] by value, re-boxing each element into a [`Value`].
///
/// A real enum rather than a boxed trait object: the element storage is a
/// different `Vec<T>` per variant, so the iterator has to be a sum of those
/// thirteen cases, and writing it out keeps the iteration free of an indirect
/// call and of an allocation.
#[derive(Debug, Clone)]
pub enum ValueListIntoIter {
    /// The empty list: yields nothing.
    End,
    /// `Byte` elements.
    Byte(std::vec::IntoIter<i8>),
    /// `Short` elements.
    Short(std::vec::IntoIter<i16>),
    /// `Int` elements.
    Int(std::vec::IntoIter<i32>),
    /// `Long` elements.
    Long(std::vec::IntoIter<i64>),
    /// `Float` elements.
    Float(std::vec::IntoIter<f32>),
    /// `Double` elements.
    Double(std::vec::IntoIter<f64>),
    /// `ByteArray` elements.
    ByteArray(std::vec::IntoIter<Vec<u8>>),
    /// `String` elements.
    String(std::vec::IntoIter<BString>),
    /// `List` elements.
    List(std::vec::IntoIter<ValueList>),
    /// `Compound` elements.
    Compound(std::vec::IntoIter<Compound>),
    /// `IntArray` elements.
    IntArray(std::vec::IntoIter<Vec<i32>>),
    /// `LongArray` elements.
    LongArray(std::vec::IntoIter<Vec<i64>>),
}

impl Iterator for ValueListIntoIter {
    type Item = Value;

    fn next(&mut self) -> Option<Value> {
        match self {
            Self::End => None,
            Self::Byte(it) => it.next().map(Value::Byte),
            Self::Short(it) => it.next().map(Value::Short),
            Self::Int(it) => it.next().map(Value::Int),
            Self::Long(it) => it.next().map(Value::Long),
            Self::Float(it) => it.next().map(Value::Float),
            Self::Double(it) => it.next().map(Value::Double),
            Self::ByteArray(it) => it.next().map(Value::ByteArray),
            Self::String(it) => it.next().map(Value::String),
            Self::List(it) => it.next().map(Value::List),
            Self::Compound(it) => it.next().map(Value::Compound),
            Self::IntArray(it) => it.next().map(Value::IntArray),
            Self::LongArray(it) => it.next().map(Value::LongArray),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::End => (0, Some(0)),
            Self::Byte(it) => it.size_hint(),
            Self::Short(it) => it.size_hint(),
            Self::Int(it) => it.size_hint(),
            Self::Long(it) => it.size_hint(),
            Self::Float(it) => it.size_hint(),
            Self::Double(it) => it.size_hint(),
            Self::ByteArray(it) => it.size_hint(),
            Self::String(it) => it.size_hint(),
            Self::List(it) => it.size_hint(),
            Self::Compound(it) => it.size_hint(),
            Self::IntArray(it) => it.size_hint(),
            Self::LongArray(it) => it.size_hint(),
        }
    }
}

impl ExactSizeIterator for ValueListIntoIter {}

impl IntoIterator for ValueList {
    type Item = Value;
    type IntoIter = ValueListIntoIter;

    fn into_iter(self) -> ValueListIntoIter {
        match self {
            Self::End => ValueListIntoIter::End,
            Self::Byte(v) => ValueListIntoIter::Byte(v.into_iter()),
            Self::Short(v) => ValueListIntoIter::Short(v.into_iter()),
            Self::Int(v) => ValueListIntoIter::Int(v.into_iter()),
            Self::Long(v) => ValueListIntoIter::Long(v.into_iter()),
            Self::Float(v) => ValueListIntoIter::Float(v.into_iter()),
            Self::Double(v) => ValueListIntoIter::Double(v.into_iter()),
            Self::ByteArray(v) => ValueListIntoIter::ByteArray(v.into_iter()),
            Self::String(v) => ValueListIntoIter::String(v.into_iter()),
            Self::List(v) => ValueListIntoIter::List(v.into_iter()),
            Self::Compound(v) => ValueListIntoIter::Compound(v.into_iter()),
            Self::IntArray(v) => ValueListIntoIter::IntArray(v.into_iter()),
            Self::LongArray(v) => ValueListIntoIter::LongArray(v.into_iter()),
        }
    }
}

macro_rules! impl_list_from {
    ($($ty: ty => $variant: ident),+) => {
        $(
            impl From<Vec<$ty>> for ValueList {
                #[inline]
                fn from(value: Vec<$ty>) -> ValueList {
                    ValueList::$variant(value)
                }
            }
        )+
    }
}

// One per element type that has an unambiguous Rust spelling. `Vec<Vec<u8>>`,
// `Vec<Vec<i32>>` and `Vec<Vec<i64>>` are deliberately absent: a `Vec<Vec<u8>>`
// is just as plausibly a list of `ByteArray`s as a list of `List`s of `Byte`,
// and guessing wrong would silently change the tag on the wire. Name those
// variants directly.
impl_list_from!(
    i8 => Byte,
    i16 => Short,
    i32 => Int,
    i64 => Long,
    f32 => Float,
    f64 => Double,
    BString => String,
    Compound => Compound,
    ValueList => List
);

/// Total equality, element by element, with [`Value`]'s float rules (see
/// [`float_eq`]).
///
/// Two lists of *different* element types are never equal, and that includes
/// [`ValueList::End`] versus any empty typed list: they encode to different
/// bytes, so calling them equal would hide a real difference.
impl PartialEq<ValueList> for ValueList {
    fn eq(&self, rhs: &ValueList) -> bool {
        match (self, rhs) {
            (Self::End, Self::End) => true,
            (Self::Byte(lhs), Self::Byte(rhs)) => lhs == rhs,
            (Self::Short(lhs), Self::Short(rhs)) => lhs == rhs,
            (Self::Int(lhs), Self::Int(rhs)) => lhs == rhs,
            (Self::Long(lhs), Self::Long(rhs)) => lhs == rhs,
            // The two float arms are why this impl is written out rather than
            // derived: `f32`/`f64` have no reflexive `==`, so `Vec`'s own
            // equality would make a list holding a `NaN` unequal to itself and
            // `Eq` unsound.
            (Self::Float(lhs), Self::Float(rhs)) => {
                lhs.len() == rhs.len() && std::iter::zip(lhs, rhs).all(|(l, r)| float_eq(*l, *r))
            }
            (Self::Double(lhs), Self::Double(rhs)) => {
                lhs.len() == rhs.len() && std::iter::zip(lhs, rhs).all(|(l, r)| float_eq(*l, *r))
            }
            (Self::ByteArray(lhs), Self::ByteArray(rhs)) => lhs == rhs,
            (Self::String(lhs), Self::String(rhs)) => lhs == rhs,
            (Self::List(lhs), Self::List(rhs)) => lhs == rhs,
            (Self::Compound(lhs), Self::Compound(rhs)) => lhs == rhs,
            (Self::IntArray(lhs), Self::IntArray(rhs)) => lhs == rhs,
            (Self::LongArray(lhs), Self::LongArray(rhs)) => lhs == rhs,
            _ => false,
        }
    }
}

/// `ValueList`'s equality is total, exactly as [`Value`]'s is, so it is `Eq`
/// too and a list can key a `HashMap`/`HashSet`.
impl Eq for ValueList {}

impl Hash for ValueList {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        // The element type leads, so that two *empty* lists of different types
        // — and in particular `End` and `Int([])`, which `PartialEq` keeps
        // apart — do not all collapse onto the same hash. The element count
        // follows it, which is what makes a *nested* list self-delimiting.
        state.write_u8(self.element_type() as u8);
        state.write_usize(self.len());
        match self {
            Self::End => {}
            // Fixed-width elements: the count above already separates them.
            Self::Byte(v) => i8::hash_slice(v, state),
            Self::Short(v) => i16::hash_slice(v, state),
            Self::Int(v) => i32::hash_slice(v, state),
            Self::Long(v) => i64::hash_slice(v, state),
            // Normalised, so that lists which `float_eq` calls equal hash alike.
            Self::Float(v) => v.iter().for_each(|f| hash_f32(*f, state)),
            Self::Double(v) => v.iter().for_each(|f| hash_f64(*f, state)),
            // Variable-length elements: each one is written length first, or the
            // payloads would run together. See `hash_bytes`.
            Self::ByteArray(v) => v.iter().for_each(|b| hash_bytes(b, state)),
            Self::String(v) => v.iter().for_each(|s| hash_bytes(s.as_slice(), state)),
            Self::List(v) => Self::hash_slice(v, state),
            Self::Compound(v) => v.iter().for_each(|map| hash_compound(map, state)),
            Self::IntArray(v) => v.iter().for_each(|a| hash_len_prefixed(a, state)),
            Self::LongArray(v) => v.iter().for_each(|a| hash_len_prefixed(a, state)),
        }
    }
}

/// Wraps a typed list into a [`Value::List`]. The payload *is* the list, so
/// nothing is copied or re-tagged.
impl From<ValueList> for Value {
    #[inline]
    fn from(value: ValueList) -> Value {
        Value::List(value)
    }
}

/// Compares a [`Value`] against a bare [`ValueList`], the way the [`Compound`]
/// impls above let one be compared against a bare map. A `Value` that is not a
/// list is never equal to one.
impl PartialEq<ValueList> for Value {
    #[inline]
    fn eq(&self, rhs: &ValueList) -> bool {
        self.as_list() == Some(rhs)
    }
}

impl PartialEq<ValueList> for &Value {
    #[inline]
    fn eq(&self, rhs: &ValueList) -> bool {
        self.as_list() == Some(rhs)
    }
}

impl PartialEq<ValueList> for &mut Value {
    #[inline]
    fn eq(&self, rhs: &ValueList) -> bool {
        self.as_list() == Some(rhs)
    }
}
