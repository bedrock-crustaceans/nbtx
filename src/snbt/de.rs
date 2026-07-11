use serde::Deserialize;
use serde::de::value::BytesDeserializer;
use serde::de::{self, Visitor};

use crate::error::{
    ExpectedNumber, ParseFloatError, ParseIntError, UnexpectedEof, UnexpectedSymbol,
    UnexpectedType, Unsupported,
};
use crate::{Error, FieldType};

/// Reads a single object of type `T` from the given string in SNBT format.
///
/// # Example
///
/// ```rust
/// # fn main() -> Result<(), nbtx::Error> {
///  #[derive(serde::Serialize, serde::Deserialize, Debug)]
///  struct Data {
///     value: String
///  }
///
/// let data = Data {
///    value: String::from("Hello, World!")
/// };
/// let out = nbtx::to_string(&data)?;
/// let data: Data = nbtx::from_string(out)?;
///
/// println!("Got {data:?}!");
/// # Ok(())
/// # }
/// ```
pub fn from_string<'a, T: Deserialize<'a>, S: AsRef<str>>(input: S) -> Result<T, Error> {
    let mut de = Deserializer::new(input.as_ref());
    T::deserialize(&mut de)
}

#[derive(Debug)]
pub struct Deserializer<'re> {
    input: &'re str,
    curr_key: Option<String>,
    #[cfg(feature = "error-context")]
    start_size: usize,
    is_key: bool,
}

impl<'re> Deserializer<'re> {
    pub fn new(input: &'re str) -> Self {
        Self {
            input,
            curr_key: None,
            #[cfg(feature = "error-context")]
            start_size: input.len(),
            is_key: false,
        }
    }

    #[cfg(feature = "error-context")]
    fn current_index(&self) -> usize {
        self.start_size - self.input.len()
    }

    fn skip(&mut self, n: usize) -> Result<(), Error> {
        if self.input.len() < n {
            return Err(Error::UnexpectedEof(UnexpectedEof {
                #[cfg(feature = "error-context")]
                at: self
                    .curr_key
                    .take()
                    .unwrap_or_else(|| String::from("unknown")),
                #[cfg(feature = "error-context")]
                index: Some(self.current_index()),
            }));
        }

        self.input = &self.input[n..];
        Ok(())
    }

    /// Skips over all whitespace and newlines and returns the next character without advancing
    /// the cursor.
    fn peek_char(&mut self) -> Result<char, Error> {
        self.input
            .chars()
            .find(|&c| c != ' ' && c != '\n')
            .ok_or(Error::UnexpectedEof(UnexpectedEof {
                #[cfg(feature = "error-context")]
                at: self
                    .curr_key
                    .take()
                    .unwrap_or_else(|| String::from("unknown")),
                #[cfg(feature = "error-context")]
                index: Some(self.current_index()),
            }))
    }

    /// Skips over all whitespace and newlines and returns the next character without advancing
    /// the cursor. This function also returns the index of the character.
    fn peek_char_with_index(&mut self) -> Result<(usize, char), Error> {
        self.input
            .char_indices()
            .find(|(_, c)| *c != ' ' && *c != '\n')
            .ok_or(Error::UnexpectedEof(UnexpectedEof {
                #[cfg(feature = "error-context")]
                at: self
                    .curr_key
                    .take()
                    .unwrap_or_else(|| String::from("unknown")),
                #[cfg(feature = "error-context")]
                index: Some(self.current_index()),
            }))
    }

    /// Returns the next non-whitespace character, advancing the cursor.
    fn next_char(&mut self) -> Result<char, Error> {
        let (idx, ch) = self.peek_char_with_index()?;
        self.skip(idx + ch.len_utf8())?;
        Ok(ch)
    }

    /// If `is_key` is set to true it will not require quotation marks.
    fn parse_string(&mut self) -> Result<&str, Error> {
        let (idx, ch) = self.peek_char_with_index()?;
        if ch == '"' {
            let first_quote = self.next_char()?;
            if first_quote != '"' {
                return Err(Error::UnexpectedSymbol(UnexpectedSymbol {
                    found: first_quote,
                    expected: Some('"'),
                    #[cfg(feature = "error-context")]
                    at: self
                        .curr_key
                        .take()
                        .unwrap_or_else(|| String::from("unknown")),
                    #[cfg(feature = "error-context")]
                    index: Some(self.current_index()),
                }));
            }

            match self.input.find('"') {
                Some(len) => {
                    let s = &self.input[..len];
                    self.input = &self.input[len + 1..];
                    Ok(s)
                }
                None => Err(Error::UnexpectedEof(UnexpectedEof {
                    #[cfg(feature = "error-context")]
                    at: self
                        .curr_key
                        .take()
                        .unwrap_or_else(|| String::from("unknown")),
                    #[cfg(feature = "error-context")]
                    index: Some(self.current_index()),
                })),
            }
        } else if self.is_key {
            // continue until colon
            match self.input.find(':') {
                Some(len) => {
                    let s = &self.input[idx..len];
                    self.input = &self.input[len..];
                    Ok(s)
                }
                None => Err(Error::UnexpectedEof(UnexpectedEof {
                    #[cfg(feature = "error-context")]
                    at: self
                        .curr_key
                        .take()
                        .unwrap_or_else(|| String::from("unknown")),
                    #[cfg(feature = "error-context")]
                    index: Some(self.current_index()),
                })),
            }
        } else {
            Err(Error::UnexpectedSymbol(UnexpectedSymbol {
                found: self.peek_char()?,
                expected: Some('"'),
                #[cfg(feature = "error-context")]
                at: self
                    .curr_key
                    .take()
                    .unwrap_or_else(|| String::from("unknown")),
                #[cfg(feature = "error-context")]
                index: Some(self.current_index()),
            }))
        }
    }

    /// Peeks at an upcoming typed-array literal (`[B;`, `[I;`, `[L;`) and
    /// returns its type character without consuming any input. Returns `None`
    /// for a plain list `[` or anything else.
    fn peek_array_marker(&self) -> Option<char> {
        let mut it = self.input.chars().filter(|&c| c != ' ' && c != '\n');
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

    /// Parses a byte-array literal `[B;1b,2b,...]` into its raw bytes. Assumes
    /// [`peek_array_marker`](Self::peek_array_marker) already confirmed a `B`
    /// typed array.
    fn parse_byte_array(&mut self) -> Result<Vec<u8>, Error> {
        // Consume the `[B;` prefix.
        self.next_char()?; // '['
        self.next_char()?; // 'B'
        let semi = self.next_char()?;
        if semi != ';' {
            return Err(Error::UnexpectedSymbol(UnexpectedSymbol {
                found: semi,
                expected: Some(';'),
                #[cfg(feature = "error-context")]
                at: self
                    .curr_key
                    .take()
                    .unwrap_or_else(|| String::from("unknown")),
                #[cfg(feature = "error-context")]
                index: Some(self.current_index()),
            }));
        }

        let mut out = Vec::new();
        loop {
            match self.peek_char()? {
                ']' => {
                    self.next_char()?;
                    break;
                }
                ',' => {
                    self.next_char()?;
                }
                _ => out.push(self.parse_array_byte()?),
            }
        }

        Ok(out)
    }

    /// Parses a single signed byte element (e.g. `62b`, `-1b`, or `7`) of a
    /// byte-array literal, returning its raw bit pattern.
    fn parse_array_byte(&mut self) -> Result<u8, Error> {
        let (start, _) = self.peek_char_with_index()?;
        let end = self.input[start..]
            .find([',', ']', ' ', '\n'])
            .map_or(self.input.len(), |i| start + i);

        let token = &self.input[start..end];
        let digits = token.strip_suffix(['b', 'B']).unwrap_or(token);
        let parsed = digits.parse::<i8>().map_err(|error| {
            Error::ParseIntError(ParseIntError {
                error,
                #[cfg(feature = "error-context")]
                at: self
                    .curr_key
                    .clone()
                    .unwrap_or_else(|| String::from("unknown")),
                #[cfg(feature = "error-context")]
                index: Some(self.current_index()),
            })
        })?;

        self.skip(end)?;
        Ok(parsed.cast_unsigned())
    }

    #[allow(clippy::too_many_lines)]
    fn parse_number<'de, V>(
        &mut self,
        visitor: V,
        desired: Option<FieldType>,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        let mut num_ty = FieldType::End;
        let mut last_digit = 0;
        let mut suffix_idx = 0;

        for (i, ch) in self.input.char_indices() {
            let lower = ch.to_ascii_lowercase();
            let ty = match lower {
                'b' => Some(FieldType::Byte),
                's' => Some(FieldType::Short),
                'l' => Some(FieldType::Long),
                'f' => Some(FieldType::Float),
                'd' => Some(FieldType::Double),
                _ => None,
            };

            if let Some(ty) = ty {
                num_ty = ty;
                suffix_idx = i;
                break;
            }

            if ch.is_ascii_digit() {
                last_digit = i;
            }

            if ch == ',' || ch == ']' || ch == '}' {
                suffix_idx = last_digit + 1;
                num_ty = FieldType::Int;
                break;
            }
        }

        // Seems like there was no number suffix or an end to the compound or array??
        if num_ty == FieldType::End {
            return Err(Error::UnexpectedEof(UnexpectedEof {
                #[cfg(feature = "error-context")]
                at: self
                    .curr_key
                    .take()
                    .unwrap_or_else(|| String::from("unknown")),
                #[cfg(feature = "error-context")]
                index: Some(self.current_index()),
            }));
        }

        if let Some(ty) = desired
            && ty != num_ty
        {
            return Err(Error::UnexpectedType(UnexpectedType {
                expected: ty,
                actual: num_ty,
                #[cfg(feature = "error-context")]
                at: self
                    .curr_key
                    .take()
                    .unwrap_or_else(|| String::from("unknown")),
                #[cfg(feature = "error-context")]
                index: Some(self.current_index()),
            }));
        }

        let (first_num_char, _) = self.peek_char_with_index()?;
        let num_str = &self.input[first_num_char..suffix_idx];

        // Skip over number and that pesky suffix if it exists
        self.skip(suffix_idx + usize::from(num_ty != FieldType::Int))?;

        match num_ty {
            FieldType::Byte => {
                let parsed = num_str.parse::<i8>().map_err(|error| {
                    Error::ParseIntError(ParseIntError {
                        error,
                        #[cfg(feature = "error-context")]
                        at: self
                            .curr_key
                            .take()
                            .unwrap_or_else(|| String::from("unknown")),
                        #[cfg(feature = "error-context")]
                        index: Some(self.current_index()),
                    })
                })?;
                visitor.visit_i8(parsed)
            }
            FieldType::Short => {
                let parsed = num_str.parse::<i16>().map_err(|error| {
                    Error::ParseIntError(ParseIntError {
                        error,
                        #[cfg(feature = "error-context")]
                        at: self
                            .curr_key
                            .take()
                            .unwrap_or_else(|| String::from("unknown")),
                        #[cfg(feature = "error-context")]
                        index: Some(self.current_index()),
                    })
                })?;
                visitor.visit_i16(parsed)
            }
            FieldType::Int => {
                let parsed = num_str.parse::<i32>().map_err(|error| {
                    Error::ParseIntError(ParseIntError {
                        error,
                        #[cfg(feature = "error-context")]
                        at: self
                            .curr_key
                            .take()
                            .unwrap_or_else(|| String::from("unknown")),
                        #[cfg(feature = "error-context")]
                        index: Some(self.current_index()),
                    })
                })?;
                visitor.visit_i32(parsed)
            }
            FieldType::Long => {
                let parsed = num_str.parse::<i64>().map_err(|error| {
                    Error::ParseIntError(ParseIntError {
                        error,
                        #[cfg(feature = "error-context")]
                        at: self
                            .curr_key
                            .take()
                            .unwrap_or_else(|| String::from("unknown")),
                        #[cfg(feature = "error-context")]
                        index: Some(self.current_index()),
                    })
                })?;
                visitor.visit_i64(parsed)
            }
            FieldType::Float => {
                let parsed = num_str.parse::<f32>().map_err(|error| {
                    Error::ParseFloatError(ParseFloatError {
                        error,
                        #[cfg(feature = "error-context")]
                        at: self
                            .curr_key
                            .take()
                            .unwrap_or_else(|| String::from("unknown")),
                        #[cfg(feature = "error-context")]
                        index: Some(self.current_index()),
                    })
                })?;
                visitor.visit_f32(parsed)
            }
            FieldType::Double => {
                let parsed = num_str.parse::<f64>().map_err(|error| {
                    Error::ParseFloatError(ParseFloatError {
                        error,
                        #[cfg(feature = "error-context")]
                        at: self
                            .curr_key
                            .take()
                            .unwrap_or_else(|| String::from("unknown")),
                        #[cfg(feature = "error-context")]
                        index: Some(self.current_index()),
                    })
                })?;
                visitor.visit_f64(parsed)
            }
            _ => unreachable!("Non-number field type {num_ty:?} encountered"),
        }
    }
}

impl<'de> de::Deserializer<'de> for &mut Deserializer<'_> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        if self.is_key {
            return self.deserialize_string(visitor);
        }

        match self.peek_char()? {
            '{' => self.deserialize_map(visitor),
            '[' if self.peek_array_marker() == Some('B') => {
                // A byte-array literal is surfaced through a newtype struct so a
                // self-describing target (`Value`) reconstructs it as a
                // `Value::ByteArray` rather than a list of bytes.
                //
                // Note: foreign self-describing value types whose visitors do
                // not implement `Visitor::visit_newtype_struct` will error on
                // byte-array literals here; `nbtx::Value` handles it.
                let bytes = self.parse_byte_array()?;
                visitor.visit_newtype_struct(BytesDeserializer::new(&bytes))
            }
            '[' if matches!(self.peek_array_marker(), Some('I' | 'L')) => {
                // `[I;...]` / `[L;...]` parsing is not implemented yet; give a
                // clearer error than an unexpected-symbol failure on 'I'/'L'.
                Err(Error::Other(String::from(
                    "parsing `[I;...]` and `[L;...]` typed array literals is not yet supported",
                )))
            }
            '[' => self.deserialize_seq(visitor),
            '0'..='9' => self.parse_number(visitor, None),
            '"' => self.deserialize_string(visitor),
            other => Err(Error::UnexpectedSymbol(UnexpectedSymbol {
                found: other,
                expected: None,
                #[cfg(feature = "error-context")]
                at: self
                    .curr_key
                    .take()
                    .unwrap_or_else(|| String::from("unknown")),
                #[cfg(feature = "error-context")]
                index: Some(self.current_index()),
            })),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        let byte = self.next_char()?.to_digit(10).ok_or_else(|| {
            Error::ExpectedNumber(ExpectedNumber {
                #[cfg(feature = "error-context")]
                at: self
                    .curr_key
                    .take()
                    .unwrap_or_else(|| String::from("unknown")),
                #[cfg(feature = "error-context")]
                index: Some(self.current_index()),
            })
        })?;

        visitor.visit_bool(byte == 1)
    }

    fn is_human_readable(&self) -> bool {
        true
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.parse_number(visitor, Some(FieldType::Byte))
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.parse_number(visitor, Some(FieldType::Short))
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.parse_number(visitor, Some(FieldType::Int))
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.parse_number(visitor, Some(FieldType::Long))
    }

    fn deserialize_u8<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_u16<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_u32<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_u64<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_f32<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_f64<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_char<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported(Unsupported {
            op: "deserializing `char` is not supported",
            #[cfg(feature = "error-context")]
            at: self
                .curr_key
                .take()
                .unwrap_or_else(|| String::from("unknown")),
            #[cfg(feature = "error-context")]
            index: Some(self.current_index()),
        }))
    }

    fn deserialize_str<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported(Unsupported {
            op: "deserializing string references is not supported",
            #[cfg(feature = "error-context")]
            at: self
                .curr_key
                .take()
                .unwrap_or_else(|| String::from("unknown")),
            #[cfg(feature = "error-context")]
            index: Some(self.current_index()),
        }))
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let v = self.parse_string()?.to_owned();
        if self.is_key {
            self.curr_key = Some(v.clone());
        }

        visitor.visit_string(v)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        // A byte-array literal (`[B;...]`) is served as raw bytes so that
        // byte-oriented targets such as `NbtByteArray`/`serde_bytes` round-trip.
        if !self.is_key && self.peek_array_marker() == Some('B') {
            let bytes = self.parse_byte_array()?;
            return visitor.visit_byte_buf(bytes);
        }

        // SNBT is a text format, so a byte-oriented target (e.g. `bstr::BString`
        // or a `Value`'s string/key) is served the raw bytes of the parsed
        // string. Any escaping/lossy behaviour is inherited from the text form.
        let v = self.parse_string()?.to_owned();
        if self.is_key {
            self.curr_key = Some(v.clone());
        }
        visitor.visit_byte_buf(v.into_bytes())
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_unit<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(ArrayDeserializer::from(self))
    }

    fn deserialize_tuple<V>(self, _len: usize, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_map(MapDeserializer::from(self))
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
}

struct MapDeserializer<'a, 're> {
    de: &'a mut Deserializer<'re>,
    first: bool,
}

impl<'a, 're> From<&'a mut Deserializer<'re>> for MapDeserializer<'a, 're> {
    fn from(de: &'a mut Deserializer<'re>) -> Self {
        Self { de, first: true }
    }
}

impl<'de> de::MapAccess<'de> for MapDeserializer<'_, '_> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        let first_char = self.de.next_char()?;
        if self.first && first_char != '{' {
            return Err(Error::UnexpectedSymbol(UnexpectedSymbol {
                found: first_char,
                expected: Some('{'),
                #[cfg(feature = "error-context")]
                at: self
                    .de
                    .curr_key
                    .take()
                    .unwrap_or_else(|| String::from("unknown")),
                #[cfg(feature = "error-context")]
                index: Some(self.de.current_index()),
            }));
        } else if first_char == '}' {
            // Map finished
            return Ok(None);
        } else if !self.first && first_char != ',' {
            return Err(Error::UnexpectedSymbol(UnexpectedSymbol {
                found: first_char,
                expected: Some(','),
                #[cfg(feature = "error-context")]
                at: self
                    .de
                    .curr_key
                    .take()
                    .unwrap_or_else(|| String::from("unknown")),
                #[cfg(feature = "error-context")]
                index: Some(self.de.current_index()),
            }));
        }

        // Check if map is empty
        if self.de.peek_char()? == '}' {
            return Ok(None);
        }

        self.de.is_key = true;
        let key = seed.deserialize(&mut *self.de).map(Some);
        self.de.is_key = false;
        self.first = false;

        let colon = self.de.next_char()?;
        if colon != ':' {
            return Err(Error::UnexpectedSymbol(UnexpectedSymbol {
                found: colon,
                expected: Some(':'),
                #[cfg(feature = "error-context")]
                at: self
                    .de
                    .curr_key
                    .take()
                    .unwrap_or_else(|| String::from("unknown")),
                #[cfg(feature = "error-context")]
                index: Some(self.de.current_index()),
            }));
        }

        key
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        seed.deserialize(&mut *self.de)
    }
}

struct ArrayDeserializer<'a, 're> {
    de: &'a mut Deserializer<'re>,
    first: bool,
}

impl<'a, 're> From<&'a mut Deserializer<'re>> for ArrayDeserializer<'a, 're> {
    fn from(de: &'a mut Deserializer<'re>) -> Self {
        Self { de, first: true }
    }
}

impl<'de> de::SeqAccess<'de> for ArrayDeserializer<'_, '_> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let first_char = self.de.next_char()?;
        if self.first && first_char != '[' {
            return Err(Error::UnexpectedSymbol(UnexpectedSymbol {
                found: first_char,
                expected: Some('['),
                #[cfg(feature = "error-context")]
                at: self
                    .de
                    .curr_key
                    .take()
                    .unwrap_or_else(|| String::from("unknown")),
                #[cfg(feature = "error-context")]
                index: Some(self.de.current_index()),
            }));
        } else if first_char == ']' {
            return Ok(None);
        } else if !self.first && first_char != ',' {
            return Err(Error::UnexpectedSymbol(UnexpectedSymbol {
                found: first_char,
                expected: Some(','),
                #[cfg(feature = "error-context")]
                at: self
                    .de
                    .curr_key
                    .take()
                    .unwrap_or_else(|| String::from("unknown")),
                #[cfg(feature = "error-context")]
                index: Some(self.de.current_index()),
            }));
        }

        // Check whether the array is empty
        if self.de.peek_char()? == ']' {
            return Ok(None);
        }

        self.first = false;

        seed.deserialize(&mut *self.de).map(Some)
    }
}
