//! Stringified NBT (SNBT) deserialization, driven by `facet` reflection.
//!
//! A recursive-descent parser reads the self-describing SNBT text; the target
//! Rust type decides how each node is stored (mirroring the binary
//! deserializer). The grammar is tolerant: arbitrary whitespace/newlines,
//! trailing commas, and both upper- and lower-case type suffixes are accepted.
//! Typed array literals `[B;..]`, `[I;..]` and `[L;..]` are supported in
//! addition to plain `[..]` lists. A bare token only becomes a number when it
//! really parses as one — `sand` is a string, not a malformed byte.
//!
//! Struct targets follow the binary codec's rules: a compound key that matches
//! no field is an [`Error::UnknownField`] unless the struct carries
//! `#[facet(nbtx::allow_unknown_fields)]`. Enum targets do too: the mandatory
//! `#[facet(nbtx::variant_as(<mode>))]` decides whether a variant is read from a
//! string naming it or from an integer literal holding its discriminant.
//!
//! [`lenient_width`](crate::Attr::LenientWidth) is honoured here as well, though
//! it has less to do than in the binary codec: a literal's type suffix is
//! dropped before parsing, so `3b` already lands in an `i32` field unaided. What
//! the attribute adds is the *conversion* — an integer literal that overflows
//! the declared type, or a decimal one where an integer is declared, is widened
//! in through [`crate::reflect`] (and range-checked there) instead of failing to
//! parse. The tag a literal counts as comes from its own suffix, exactly as for
//! an untyped node: `3b` is a `Byte`, `3` an `Int`, `3.0` a `Double`, `3.0f` a
//! `Float`.
//!
//! That has a sharp edge worth calling out on its own: `f32` and `f64` are
//! *not* interchangeable declarations here, because a bare (suffixless)
//! decimal literal is always a `Double`. Given `#[facet(nbtx::lenient_width(f32))]`
//! on an integer field, `3.0f` widens in (it is a `Float`, the tag named) but a
//! bare `3.0` does not (it is a `Double`, which was never named) — the
//! opposite of what the same declaration means for the *binary* codec, where
//! there is no such thing as a suffix. Naming both `f32` and `f64` accepts
//! either spelling.

use crate::value::Compound;
use bstr::BString;
use facet::Facet;
use facet_core::{Def, ScalarType, Shape, Type, UserType};
use facet_reflect::Partial;

use crate::error::{ParseFloatError, ParseIntError, UnexpectedEof, UnexpectedSymbol};
// Shared with the binary codec and the `Value` conversion; see `crate::reflect`.
use crate::reflect::{
    Lenient, VariantAs, WireScalar, is_bstring, is_value, lenient_discriminant, reflect_err,
    set_lenient, set_wire, unknown_field, unsupported, wire_scalar,
};
use crate::{Error, FieldType, Value, check_depth};

type Part<'f> = Partial<'f, true>;

const DELIMS: &[char] = &[' ', '\n', '\t', '\r', ',', ':', '{', '}', '[', ']', '"'];

/// Which kind of nested (recursive) container starts at the parser's cursor.
///
/// Typed arrays are deliberately absent: `[B;..]`/`[I;..]`/`[L;..]` hold scalars
/// only, so they are leaves for the purposes of the depth guard.
#[derive(Clone, Copy)]
enum Nested {
    Compound,
    List,
}

fn eof() -> Error {
    Error::UnexpectedEof(UnexpectedEof {
        #[cfg(feature = "error-context")]
        at: String::from("unknown"),
        #[cfg(feature = "error-context")]
        index: None,
    })
}

fn unexpected(found: char, expected: Option<char>) -> Error {
    Error::UnexpectedSymbol(UnexpectedSymbol {
        found,
        expected,
        #[cfg(feature = "error-context")]
        at: String::from("unknown"),
        #[cfg(feature = "error-context")]
        index: None,
    })
}

/// Reads a single value of type `T` from an SNBT string.
pub fn from_string<'f, T: Facet<'f>>(input: impl AsRef<str>) -> Result<T, Error> {
    let mut de = Deserializer::new(input.as_ref());
    de.parse()
}

/// An SNBT deserializer.
#[derive(Debug)]
pub struct Deserializer<'a> {
    input: &'a str,
}

impl<'a> Deserializer<'a> {
    /// Creates a new deserializer over the given SNBT text.
    #[must_use]
    pub fn new(input: &'a str) -> Self {
        Self { input }
    }

    /// Parses the input into a value of type `T`.
    pub fn parse<'f, T: Facet<'f>>(&mut self) -> Result<T, Error> {
        let p = Partial::alloc::<T>().map_err(reflect_err)?;
        let p = self.parse_into(p, <T as Facet>::SHAPE, 0, Lenient::NONE)?;
        p.build()
            .map_err(reflect_err)?
            .materialize()
            .map_err(reflect_err)
    }

    fn skip_ws(&mut self) {
        self.input = self.input.trim_start_matches([' ', '\n', '\t', '\r']);
    }

    /// Returns the next non-whitespace character without consuming it.
    fn peek(&mut self) -> Result<char, Error> {
        self.skip_ws();
        self.input.chars().next().ok_or_else(eof)
    }

    /// Consumes and returns the next non-whitespace character.
    fn bump(&mut self) -> Result<char, Error> {
        let c = self.peek()?;
        self.input = &self.input[c.len_utf8()..];
        Ok(c)
    }

    fn expect(&mut self, expected: char) -> Result<(), Error> {
        let c = self.bump()?;
        if c == expected {
            Ok(())
        } else {
            Err(unexpected(c, Some(expected)))
        }
    }

    /// Reads a bare token up to the next delimiter (whitespace or structural
    /// character).
    fn read_token(&mut self) -> Result<&'a str, Error> {
        self.skip_ws();
        let end = self.input.find(DELIMS).unwrap_or(self.input.len());
        if end == 0 {
            return Err(unexpected(self.input.chars().next().unwrap_or(' '), None));
        }
        let tok = &self.input[..end];
        self.input = &self.input[end..];
        Ok(tok)
    }

    /// Reads a quoted string, un-escaping `\"` and `\\` (the inverse of the
    /// serializer's escaping). An unknown escape keeps the following character
    /// literally.
    fn read_quoted(&mut self) -> Result<String, Error> {
        self.expect('"')?;
        let mut out = String::new();
        let mut chars = self.input.char_indices();
        loop {
            let (i, c) = chars.next().ok_or_else(eof)?;
            match c {
                '"' => {
                    // Consume through the closing quote (`"` is one byte).
                    self.input = &self.input[i + 1..];
                    return Ok(out);
                }
                '\\' => {
                    let (_, esc) = chars.next().ok_or_else(eof)?;
                    out.push(esc);
                }
                other => out.push(other),
            }
        }
    }

    /// Reads a string value or key (quoted or bare), returning an owned string
    /// because quoted strings may contain escapes.
    fn read_string(&mut self) -> Result<String, Error> {
        if self.peek()? == '"' {
            self.read_quoted()
        } else {
            Ok(self.read_token()?.to_owned())
        }
    }

    /// If the upcoming input is a typed-array literal (`[B;`/`[I;`/`[L;`),
    /// returns its marker character without consuming input.
    fn array_marker(&self) -> Option<char> {
        let mut it = self.input.chars().filter(|&c| !c.is_whitespace());
        if it.next()? != '[' {
            return None;
        }
        let ty = it.next()?;
        if matches!(ty, 'B' | 'I' | 'L') && it.next()? == ';' {
            Some(ty)
        } else {
            None
        }
    }

    /// Parses any SNBT node into a dynamic [`Value`].
    ///
    /// `depth` is the number of containers already entered;
    /// [`Self::parse_compound_value`] and [`Self::parse_list_value`] check it
    /// against [`MAX_DEPTH`](crate::MAX_DEPTH) on entry, so a pathologically
    /// nested document yields [`Error::MaxDepthExceeded`] instead of overflowing
    /// the stack (which aborts the process and cannot be caught).
    ///
    /// This function is *not* part of the recursive cycle: the two container
    /// parsers dispatch to each other directly (see [`Self::next_nested`]), so
    /// only their own frames are live at depth. Everything that is not a nested
    /// container is delegated to an `#[inline(never)]` leaf, keeping those frames
    /// small — an unoptimised build gives every temporary in a function its own
    /// never-reused slot, so the typed-array/string/token temporaries would
    /// otherwise be paid for at every level of nesting.
    fn parse_value(&mut self, depth: usize) -> Result<Value, Error> {
        match self.next_nested()? {
            Some(Nested::Compound) => self.parse_compound_value(depth),
            Some(Nested::List) => self.parse_list_value(depth),
            None => self.parse_leaf_value(),
        }
    }

    /// Reports which kind of nested container starts at the cursor, without
    /// consuming anything. `None` covers the leaves: typed arrays (which hold
    /// scalars and so never recurse), quoted strings and bare tokens.
    #[inline(never)]
    fn next_nested(&mut self) -> Result<Option<Nested>, Error> {
        match self.peek()? {
            '{' => Ok(Some(Nested::Compound)),
            '[' if self.array_marker().is_none() => Ok(Some(Nested::List)),
            _ => Ok(None),
        }
    }

    /// The non-recursive arms of [`Self::parse_value`]: `[B;..]`/`[I;..]`/`[L;..]`
    /// typed arrays, quoted strings and bare tokens. Split out (and never
    /// inlined) to keep the recursive frames small — see the note there.
    #[inline(never)]
    fn parse_leaf_value(&mut self) -> Result<Value, Error> {
        match self.peek()? {
            '[' => match self.array_marker() {
                Some('B') => Ok(Value::ByteArray(self.parse_byte_array()?)),
                Some('I') => Ok(Value::IntArray(self.parse_int_array()?)),
                // `next_nested` only routes a `[` here when `array_marker`
                // matched, so this is `L`.
                _ => Ok(Value::LongArray(self.parse_long_array()?)),
            },
            '"' => Ok(Value::String(BString::from(self.read_quoted()?))),
            _ => {
                let tok = self.read_token()?;
                token_to_value(self, tok)
            }
        }
    }

    /// Reads the key (and its `:`) of the next compound entry, or returns `None`
    /// if the closing `}` was consumed instead.
    ///
    /// Outlined from the recursive [`Self::parse_compound_value`] for the same
    /// reason as [`Self::parse_leaf_value`].
    #[inline(never)]
    fn compound_key(&mut self) -> Result<Option<BString>, Error> {
        if self.peek()? == '}' {
            self.bump()?;
            return Ok(None);
        }
        let key = self.read_string()?;
        self.expect(':')?;
        Ok(Some(BString::from(key)))
    }

    /// Consumes the separator that follows a container element: `Ok(true)` to
    /// keep going, `Ok(false)` if `close` was consumed instead. Outlined for the
    /// same reason as [`Self::compound_key`].
    #[inline(never)]
    fn element_sep(&mut self, close: char) -> Result<bool, Error> {
        let c = self.peek()?;
        if c == ',' {
            self.bump()?;
            Ok(true)
        } else if c == close {
            self.bump()?;
            Ok(false)
        } else {
            Err(unexpected(c, None))
        }
    }

    /// Checks the depth guard and consumes the container's opening bracket.
    /// Outlined so the recursive frames pay for one `Result` temporary here
    /// instead of two.
    #[inline(never)]
    fn open_container(&mut self, open: char, depth: usize) -> Result<(), Error> {
        check_depth(depth)?;
        self.expect(open)
    }

    /// Consumes `c` if it is the next non-whitespace character.
    #[inline(never)]
    fn consume_if(&mut self, c: char) -> Result<bool, Error> {
        if self.peek()? == c {
            self.bump()?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Parses a `{..}` compound. Recursive; see [`Self::parse_value`] for how the
    /// depth guard and the frame-size discipline work.
    ///
    /// Returns a whole [`Value`] rather than a bare [`Compound`] so that the
    /// caller does not need a second `Value` temporary per level of nesting.
    fn parse_compound_value(&mut self, depth: usize) -> Result<Value, Error> {
        self.open_container('{', depth)?;
        let mut map = Compound::new();
        while let Some(key) = self.compound_key()? {
            // Dispatch to the nested container directly instead of bouncing
            // through `parse_value`, so only one frame per level of nesting is
            // live for the cheapest deep document an attacker can send.
            let value = match self.next_nested()? {
                Some(Nested::Compound) => self.parse_compound_value(depth + 1)?,
                Some(Nested::List) => self.parse_list_value(depth + 1)?,
                None => self.parse_leaf_value()?,
            };
            insert_first_wins(&mut map, key, value);
            if !self.element_sep('}')? {
                break;
            }
        }
        Ok(Value::Compound(map))
    }

    /// Parses a plain `[..]` list. Recursive; see [`Self::parse_compound_value`].
    fn parse_list_value(&mut self, depth: usize) -> Result<Value, Error> {
        self.open_container('[', depth)?;
        let mut out = Vec::new();
        loop {
            if self.consume_if(']')? {
                break;
            }
            let item = match self.next_nested()? {
                Some(Nested::Compound) => self.parse_compound_value(depth + 1)?,
                Some(Nested::List) => self.parse_list_value(depth + 1)?,
                None => self.parse_leaf_value()?,
            };
            out.push(item);
            if !self.element_sep(']')? {
                break;
            }
        }
        Ok(Value::List(out))
    }

    /// Consumes a `[X;` typed-array prefix.
    fn open_typed_array(&mut self, marker: char) -> Result<(), Error> {
        self.expect('[')?;
        self.expect(marker)?;
        self.expect(';')
    }

    fn parse_byte_array(&mut self) -> Result<Vec<u8>, Error> {
        self.open_typed_array('B')?;
        let mut out = Vec::new();
        self.array_elements(|de| {
            let tok = de.read_token()?;
            out.push(parse_int::<i8>(de, tok)?.cast_unsigned());
            Ok(())
        })?;
        Ok(out)
    }

    fn parse_int_array(&mut self) -> Result<Vec<i32>, Error> {
        self.open_typed_array('I')?;
        let mut out = Vec::new();
        self.array_elements(|de| {
            let tok = de.read_token()?;
            out.push(parse_int::<i32>(de, tok)?);
            Ok(())
        })?;
        Ok(out)
    }

    fn parse_long_array(&mut self) -> Result<Vec<i64>, Error> {
        self.open_typed_array('L')?;
        let mut out = Vec::new();
        self.array_elements(|de| {
            let tok = de.read_token()?;
            out.push(parse_int::<i64>(de, tok)?);
            Ok(())
        })?;
        Ok(out)
    }

    /// Runs `f` for each element of an already-opened array until the closing
    /// `]`, tolerating a trailing comma.
    fn array_elements(
        &mut self,
        mut f: impl FnMut(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        loop {
            if self.peek()? == ']' {
                self.bump()?;
                break;
            }
            f(self)?;
            match self.peek()? {
                ',' => {
                    self.bump()?;
                }
                ']' => {
                    self.bump()?;
                    break;
                }
                other => return Err(unexpected(other, None)),
            }
        }
        Ok(())
    }

    /// Parses the next SNBT node into `p`, guided by `shape`.
    ///
    /// `depth` is the number of containers already entered; every
    /// nested-container arm checks it against [`MAX_DEPTH`](crate::MAX_DEPTH)
    /// before recursing, exactly as the binary deserializer does.
    ///
    /// `lenient` is the `#[facet(nbtx::lenient_width(...))]` declaration of the
    /// struct field this node belongs to, carried down so that one declaration
    /// widens every element of a `Vec` and the inside of an `Option` alike.
    fn parse_into<'f>(
        &mut self,
        p: Part<'f>,
        shape: &'static Shape,
        depth: usize,
        lenient: Lenient,
    ) -> Result<Part<'f>, Error> {
        if is_value(shape) {
            let v = self.parse_value(depth)?;
            return p.set(v).map_err(reflect_err);
        }

        if let Def::Option(def) = shape.def {
            let p = p.begin_some().map_err(reflect_err)?;
            let p = self.parse_into(p, def.t(), depth, lenient)?;
            return p.end().map_err(reflect_err);
        }

        // A `bstr::BString` field reads a string (quoted or bare), matching what
        // the serializer writes and what the binary codec does — *not* the
        // `Def::List<u8>` byte-array literal its reflection would suggest.
        if is_bstring(shape) {
            let s = self.read_string()?;
            return p.set(BString::from(s)).map_err(reflect_err);
        }

        if let Some(scalar) = ScalarType::try_from_shape(shape) {
            return self.parse_scalar_into(p, scalar, lenient);
        }

        match shape.def {
            Def::List(def) => return self.parse_seq_into(p, def.t(), None, depth, lenient),
            Def::Array(def) => {
                return self.parse_seq_into(p, def.t(), Some(def.n), depth, lenient);
            }
            Def::Map(def) => return self.parse_map_into(p, def.v(), depth),
            _ => {}
        }

        match shape.ty {
            Type::User(UserType::Struct(_)) => self.parse_struct_into(p, shape, depth),
            Type::User(UserType::Enum(_)) => self.parse_enum_into(p, shape),
            _ => Err(unsupported("deserialization of this type is not supported")),
        }
    }

    /// Parses a unit enum variant, in whichever form the enum's mandatory
    /// `#[facet(nbtx::variant_as(...))]` declared: a string holding its
    /// (rename-aware) name, or a suffixed integer literal holding its
    /// discriminant. See [`crate::reflect::VariantAs`].
    fn parse_enum_into<'f>(
        &mut self,
        p: Part<'f>,
        shape: &'static Shape,
    ) -> Result<Part<'f>, Error> {
        let mode = VariantAs::of(shape)?;
        let lenient = Lenient::of_enum(shape, mode)?;
        if mode == VariantAs::Str {
            let name = self.read_string()?;
            return p.select_variant_named(&name).map_err(reflect_err);
        }
        // Read at the width of the tag the mode uses, exactly as a scalar field
        // of that tag would be read, then reinterpret the bit pattern for the
        // unsigned modes.
        let tok = self.read_token()?;
        let raw = match mode.tag() {
            FieldType::Byte => parse_int::<i8>(self, tok).map(i64::from),
            FieldType::Short => parse_int::<i16>(self, tok).map(i64::from),
            FieldType::Int => parse_int::<i32>(self, tok).map(i64::from),
            _ => parse_int::<i64>(self, tok),
        };
        let disc = match raw {
            Ok(raw) => mode.widen(raw),
            // The enum's own `#[facet(nbtx::lenient_width(...))]` widens the
            // literal in, on exactly the terms a lenient scalar field gets.
            Err(err) => match self.widen_token(tok, lenient) {
                Some(wire) => lenient_discriminant(wire, mode)?,
                None => return Err(err),
            },
        };
        p.select_variant(disc).map_err(reflect_err)
    }

    fn parse_scalar_into<'f>(
        &mut self,
        p: Part<'f>,
        scalar: ScalarType,
        lenient: Lenient,
    ) -> Result<Part<'f>, Error> {
        // The three targets `lenient_width` never applies to are handled first:
        // a string is not a number at all, and `bool`/`u8` are outside the six
        // NBT scalar wire types the attribute may name.
        match scalar {
            ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
                let s = self.read_string()?;
                return p.set(s).map_err(reflect_err);
            }
            ScalarType::Bool => {
                let tok = self.read_token()?;
                let b = match tok {
                    "true" => true,
                    "false" => false,
                    _ => parse_int::<i64>(self, tok)? != 0,
                };
                return p.set(b).map_err(reflect_err);
            }
            ScalarType::U8 => {
                let tok = self.read_token()?;
                return p
                    .set(parse_int::<i8>(self, tok)?.cast_unsigned())
                    .map_err(reflect_err);
            }
            _ => {}
        }

        // SNBT is already width-tolerant by construction — `parse_int`/
        // `parse_float` drop a literal's type suffix and read at the *field's*
        // width, so `3b` lands in an `i32` field with or without the attribute.
        // So the natural parse is tried first and `lenient_width` only widens
        // what it rejects: an out-of-range integer, or a literal whose suffix
        // makes it a different kind of number than the field.
        let tok = self.read_token()?;
        let natural = match scalar {
            ScalarType::I8 => parse_int::<i8>(self, tok).map(WireScalar::Byte),
            ScalarType::I16 => parse_int::<i16>(self, tok).map(WireScalar::Short),
            ScalarType::I32 => parse_int::<i32>(self, tok).map(WireScalar::Int),
            ScalarType::I64 => parse_int::<i64>(self, tok).map(WireScalar::Long),
            ScalarType::F32 => parse_float::<f32>(self, tok).map(WireScalar::Float),
            ScalarType::F64 => parse_float::<f64>(self, tok).map(WireScalar::Double),
            _ => {
                return Err(unsupported(
                    "deserialization of this scalar type is not supported",
                ));
            }
        };
        match natural {
            Ok(wire) => set_wire(p, wire),
            Err(err) => match self.widen_token(tok, lenient) {
                Some(wire) => set_lenient(p, scalar, wire),
                None => Err(err),
            },
        }
    }

    /// Re-reads a token that the field's own width could not parse, as the
    /// [`Value`] its literal really denotes, and returns it when
    /// `#[facet(nbtx::lenient_width(...))]` accepts that tag.
    ///
    /// The tag comes from the literal's own suffix, exactly as it would for an
    /// untyped SNBT node: `3b` is a `Byte`, `3` an `Int`, `3.0` a `Double` and
    /// `3.0f` a `Float`. `None` means "nothing to widen here" and leaves the
    /// caller's original parse error in place — a bareword is still a bareword,
    /// not a number that failed to fit.
    fn widen_token(&self, tok: &str, lenient: Lenient) -> Option<WireScalar> {
        let value = token_to_value(self, tok).ok()?;
        let wire = wire_scalar(&value)?;
        lenient.accepts(wire.tag()).then_some(wire)
    }

    fn parse_seq_into<'f>(
        &mut self,
        p: Part<'f>,
        elem_shape: &'static Shape,
        array_len: Option<usize>,
        depth: usize,
        lenient: Lenient,
    ) -> Result<Part<'f>, Error> {
        check_depth(depth)?;
        // Consume the opening bracket (and a `X;` prefix for typed arrays).
        match self.array_marker() {
            Some(marker) => self.open_typed_array(marker)?,
            None => self.expect('[')?,
        }

        if let Some(n) = array_len {
            let mut p = p.init_array().map_err(reflect_err)?;
            let mut i = 0;
            loop {
                if self.peek()? == ']' {
                    self.bump()?;
                    break;
                }
                if i >= n {
                    return Err(unsupported("too many elements for fixed-size array"));
                }
                p = p.begin_nth_field(i).map_err(reflect_err)?;
                p = self.parse_into(p, elem_shape, depth + 1, lenient)?;
                p = p.end().map_err(reflect_err)?;
                i += 1;
                match self.peek()? {
                    ',' => {
                        self.bump()?;
                    }
                    ']' => {
                        self.bump()?;
                        break;
                    }
                    other => return Err(unexpected(other, None)),
                }
            }
            Ok(p)
        } else {
            let mut p = p.init_list().map_err(reflect_err)?;
            loop {
                if self.peek()? == ']' {
                    self.bump()?;
                    break;
                }
                p = p.begin_list_item().map_err(reflect_err)?;
                p = self.parse_into(p, elem_shape, depth + 1, lenient)?;
                p = p.end().map_err(reflect_err)?;
                match self.peek()? {
                    ',' => {
                        self.bump()?;
                    }
                    ']' => {
                        self.bump()?;
                        break;
                    }
                    other => return Err(unexpected(other, None)),
                }
            }
            Ok(p)
        }
    }

    fn parse_map_into<'f>(
        &mut self,
        p: Part<'f>,
        value_shape: &'static Shape,
        depth: usize,
    ) -> Result<Part<'f>, Error> {
        check_depth(depth)?;
        self.expect('{')?;
        let mut p = p.init_map().map_err(reflect_err)?;
        loop {
            if self.peek()? == '}' {
                self.bump()?;
                break;
            }
            let key = self.read_string()?;
            self.expect(':')?;
            p = p.begin_key().map_err(reflect_err)?;
            p = p.set(key).map_err(reflect_err)?;
            p = p.end().map_err(reflect_err)?;
            p = p.begin_value().map_err(reflect_err)?;
            // A map's values carry no `lenient_width` of their own: the
            // attribute is refused on a map-typed field in the first place.
            p = self.parse_into(p, value_shape, depth + 1, Lenient::NONE)?;
            p = p.end().map_err(reflect_err)?;
            match self.peek()? {
                ',' => {
                    self.bump()?;
                }
                '}' => {
                    self.bump()?;
                    break;
                }
                other => return Err(unexpected(other, None)),
            }
        }
        Ok(p)
    }

    /// Parses a `{..}` compound into a `#[derive(Facet)]` struct.
    ///
    /// An unrecognised key is an [`Error::UnknownField`] unless the struct
    /// carries `#[facet(nbtx::allow_unknown_fields)]`, in which case its value
    /// is parsed and discarded — the same rule the binary codec applies in
    /// `nbt::de::read_struct`.
    fn parse_struct_into<'f>(
        &mut self,
        p: Part<'f>,
        shape: &'static Shape,
        depth: usize,
    ) -> Result<Part<'f>, Error> {
        let Type::User(UserType::Struct(st)) = shape.ty else {
            return Err(unsupported("expected a struct"));
        };
        check_depth(depth)?;
        self.expect('{')?;
        // Same rule as the binary codec (`nbt::de::read_struct`): an unknown key
        // is an error unless the struct carries
        // `#[facet(nbtx::allow_unknown_fields)]`, so both codecs agree about the
        // same schema and schema drift cannot silently discard data.
        let allow_unknown = crate::has_nbtx_attr(shape, "allow_unknown_fields");
        let mut p = p;
        loop {
            if self.peek()? == '}' {
                self.bump()?;
                break;
            }
            let key = self.read_string()?;
            self.expect(':')?;

            let field = st
                .fields
                .iter()
                .enumerate()
                .find(|(_, f)| f.effective_name() == key.as_str());

            if let Some((idx, f)) = field {
                let lenient = Lenient::of_field(shape, f)?;
                p = p.begin_nth_field(idx).map_err(reflect_err)?;
                p = self.parse_into(p, f.shape(), depth + 1, lenient)?;
                p = p.end().map_err(reflect_err)?;
            } else if allow_unknown {
                // Opted out of strict decoding: parse and discard the value.
                let _ = self.parse_value(depth + 1)?;
            } else {
                return Err(unknown_field(shape, key.as_bytes()));
            }

            match self.peek()? {
                ',' => {
                    self.bump()?;
                }
                '}' => {
                    self.bump()?;
                    break;
                }
                other => return Err(unexpected(other, None)),
            }
        }
        Ok(p)
    }
}

/// Inserts a compound entry, keeping the **first** occurrence of a duplicate key
/// (and its position), matching the binary codec (`nbt::io::read_compound`).
///
/// A free `#[inline(never)]` function so the `Entry` machinery's temporaries do
/// not land in the recursive `parse_compound_value` frame.
#[inline(never)]
fn insert_first_wins(map: &mut Compound, key: BString, value: Value) {
    map.entry(key).or_insert(value);
}

/// Splits a numeric token into its digits and optional one-letter type suffix.
fn split_suffix(tok: &str) -> (&str, Option<char>) {
    if let Some(last) = tok.chars().last()
        && last.is_ascii_alphabetic()
        && tok.len() > 1
    {
        (&tok[..tok.len() - last.len_utf8()], Some(last))
    } else {
        (tok, None)
    }
}

fn parse_int<T: std::str::FromStr<Err = std::num::ParseIntError>>(
    _de: &Deserializer,
    tok: &str,
) -> Result<T, Error> {
    let (digits, _) = split_suffix(tok);
    digits.parse::<T>().map_err(|error| {
        Error::ParseIntError(ParseIntError {
            error,
            #[cfg(feature = "error-context")]
            at: String::from("unknown"),
            #[cfg(feature = "error-context")]
            index: None,
        })
    })
}

fn parse_float<T: std::str::FromStr<Err = std::num::ParseFloatError>>(
    _de: &Deserializer,
    tok: &str,
) -> Result<T, Error> {
    let (digits, _) = split_suffix(tok);
    digits.parse::<T>().map_err(|error| {
        Error::ParseFloatError(ParseFloatError {
            error,
            #[cfg(feature = "error-context")]
            at: String::from("unknown"),
            #[cfg(feature = "error-context")]
            index: None,
        })
    })
}

/// Converts a bareword token into a [`Value`], inferring the tag from its type
/// suffix (or lack thereof).
///
/// A one-letter suffix only *commits* the token to the number grammar when the
/// part before it actually looks like a number ([`looks_numeric`]). Otherwise
/// the token is an ordinary unquoted string, which is what vanilla's `TagParser`
/// does when its numeric regexes do not match. Without that guard every bareword
/// whose last letter happens to be a suffix — `sand`, `gold`, `red`, `glass`,
/// `oak_slab`, `diamond_sword`, and a large fraction of Minecraft's block and
/// item ids — is rejected as a malformed number.
///
/// A token that *does* look like a number but does not parse (`128b`, `1.5b`,
/// `1.2.3f`) stays an error rather than degrading to text: the suffix states the
/// intent, so silently keeping it as a string would hide a typo. That is a
/// deliberate divergence from vanilla, which catches the `NumberFormatException`
/// and returns a `StringTag`; `snbt_grammar::a_suffixed_literal_out_of_range_is_an_error`
/// pins it.
fn token_to_value(de: &Deserializer, tok: &str) -> Result<Value, Error> {
    match tok {
        "true" => return Ok(Value::Byte(1)),
        "false" => return Ok(Value::Byte(0)),
        _ => {}
    }

    let (digits, suffix) = split_suffix(tok);
    match suffix.map(|c| c.to_ascii_lowercase()) {
        Some('b') if looks_numeric(digits) => return Ok(Value::Byte(parse_int::<i8>(de, tok)?)),
        Some('s') if looks_numeric(digits) => return Ok(Value::Short(parse_int::<i16>(de, tok)?)),
        Some('l') if looks_numeric(digits) => return Ok(Value::Long(parse_int::<i64>(de, tok)?)),
        Some('f') if looks_like_float(digits) => {
            return Ok(Value::Float(parse_float::<f32>(de, tok)?));
        }
        Some('d') if looks_like_float(digits) => {
            return Ok(Value::Double(parse_float::<f64>(de, tok)?));
        }
        _ => {}
    }

    // No suffix, or a suffix on something that is not a number at all: an
    // integer is an `Int`, a decimal is a `Double`, anything else is an unquoted
    // string.
    if let Ok(i) = tok.parse::<i32>() {
        Ok(Value::Int(i))
    } else if (tok.contains('.') || tok.contains('e') || tok.contains('E'))
        && let Ok(f) = tok.parse::<f64>()
    {
        Ok(Value::Double(f))
    } else {
        Ok(Value::String(BString::from(tok.to_owned())))
    }
}

/// Does the suffix-stripped part of a token start like a number — an optional
/// sign followed by a digit or a decimal point?
///
/// This is the *syntactic* test that decides whether a suffixed token is read as
/// a number at all. It deliberately does not try to parse: `1.5b` and `128b` are
/// numbers that fail to parse (and are reported as such), while `sand` never
/// enters the number grammar in the first place.
fn looks_numeric(digits: &str) -> bool {
    let body = digits.strip_prefix(['+', '-']).unwrap_or(digits);
    body.starts_with(|c: char| c.is_ascii_digit() || c == '.')
}

/// As [`looks_numeric`], plus the spellings Rust's float `Display` emits for the
/// non-finite values — `NaN`, `inf`, `-inf`. The SNBT writer renders
/// `Value::Float(f32::NAN)` as `NaNf`, so the reader has to accept it back
/// (`snbt_grammar::float_special_values_survive_the_text_roundtrip`). The cost is
/// that a bareword like `nand` or `infd` is read as a number; that is inherent to
/// spelling the non-finite floats in words and predates the string fallback.
fn looks_like_float(digits: &str) -> bool {
    if looks_numeric(digits) {
        return true;
    }
    let body = digits.strip_prefix(['+', '-']).unwrap_or(digits);
    body.eq_ignore_ascii_case("nan")
        || body.eq_ignore_ascii_case("inf")
        || body.eq_ignore_ascii_case("infinity")
}
