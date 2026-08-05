//! The [`Named<T>`] root wrapper: an explicit opt-in to reading/writing the
//! NBT document's *root name*.

use bstr::BString;
use facet::Facet;
#[cfg(feature = "nbt")]
use facet_core::{Shape, Type, UserType};

/// A document root that carries an explicit root name.
///
/// # Why this exists
///
/// Every NBT document begins with `<tag byte><root name><payload>`. nbtx's
/// plain entry points (`to_be_bytes`, `from_be_bytes`, …) always write an
/// **empty** root name and discard the one they read. That is the right default
/// for Bedrock: essentially every Bedrock document (network packets,
/// `level.dat`, `servers.dat`) has an empty root name, and deriving one from a
/// Rust type name would make the wire format depend on Rust identifiers.
///
/// Some files do carry a meaningful root name, though — Java-Edition documents
/// such as the classic `hello world` and `Level` files. Wrap the payload in a
/// `Named` to read or write it:
///
/// ```
/// # #[cfg(feature = "nbt")] {
/// use nbtx::{Named, Value};
///
/// let doc = Named {
///     name: "hello world".into(),
///     value: Value::Int(1),
/// };
/// let bytes = nbtx::to_be_bytes(&doc).unwrap();
/// let back: Named<Value> = nbtx::from_be_bytes(&mut bytes.as_slice()).unwrap();
/// assert_eq!(back.name, doc.name);
/// assert_eq!(back.value, doc.value);
/// # }
/// ```
///
/// `Named` is only meaningful at the *root*: nested values are named by their
/// compound key, so a `Named` field inside a struct is just an ordinary
/// two-field compound.
///
/// # Detection
///
/// The codecs recognise this type by `Shape::decl_id`, facet's
/// type-parameter-erased declaration identity: every `Named<T>` shares one
/// `decl_id`, and no type declared anywhere else does. A struct of your own that
/// happens to be called `Named` and to have `name`/`value` fields is therefore
/// **not** affected — it encodes as an ordinary compound.
///
/// `Shape::id` cannot be used the way it is for [`Value`](crate::Value), because
/// `Named<T>` is generic and monomorphisation gives it a distinct `id` per `T`.
/// A namespaced `#[facet(nbtx::…)]` marker cannot be used either: Rust forbids
/// referring to a `macro_export`ed macro of the *current* crate by absolute
/// path, and facet's derive expands `nbtx::foo` to exactly such a path.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Named<T> {
    /// The document's root name. Raw bytes, like any other NBT string.
    pub name: BString,
    /// The document's root value.
    pub value: T,
}

impl<T> Named<T> {
    /// Creates a new named root.
    pub fn new(name: impl Into<BString>, value: T) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }

    /// Discards the name and returns the wrapped value.
    pub fn into_inner(self) -> T {
        self.value
    }
}

/// Where the `name`/`value` fields of a [`Named<T>`] live in its shape.
#[cfg(feature = "nbt")]
pub(crate) struct NamedRoot {
    /// Index of the `name` field.
    pub(crate) name_idx: usize,
    /// Index of the `value` field.
    pub(crate) value_idx: usize,
    /// Shape of the `value` field.
    pub(crate) value_shape: &'static Shape,
}

/// Recognises [`Named<T>`] from its shape, for *any* `T`.
///
/// `Named<T>` is generic, so monomorphisation gives it a distinct `Shape::id`
/// per `T` and it cannot be matched by id the way `Value` is. It is matched on
/// `Shape::decl_id` instead — facet's type-parameter-erased *declaration*
/// identity, which is shared by every instantiation of one declaration and by no
/// other declaration. `<Named<()>>::SHAPE` supplies the reference `decl_id`; the
/// choice of `()` is arbitrary, as any instantiation would do.
///
/// This is what keeps a *lookalike* — a user's own two-field `Named { name:
/// BString, value: T }` — out of the root-naming path: a purely structural match
/// on `type_identifier` would silently hijack it into root-name wire semantics.
/// See the note on [`Named`] for why a namespaced attribute cannot be used.
///
/// The field lookup that follows is not a second identity check (`decl_id`
/// already settled that); it just locates the two fields without assuming a
/// declaration order.
#[cfg(feature = "nbt")]
pub(crate) fn as_named_root(shape: &'static Shape) -> Option<NamedRoot> {
    if shape.decl_id != <Named<()> as Facet>::SHAPE.decl_id {
        return None;
    }
    let Type::User(UserType::Struct(st)) = shape.ty else {
        return None;
    };
    let name_idx = st.fields.iter().position(|f| f.name == "name")?;
    let value_idx = st.fields.iter().position(|f| f.name == "value")?;
    debug_assert_eq!(
        st.fields[name_idx].shape().id,
        <BString as Facet>::SHAPE.id,
        "`Named::name` must stay a `BString`"
    );
    Some(NamedRoot {
        name_idx,
        value_idx,
        value_shape: st.fields[value_idx].shape(),
    })
}
