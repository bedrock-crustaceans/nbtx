use std::ops::{Index, Range, RangeFrom};

use serde::de::{self, MapAccess, Visitor};

use crate::{Error, FieldType};

#[derive(Debug)]
pub struct Deserializer<'re> {
    input: &'re str,
    curr_key: Option<String>,
    is_key: bool,
}

impl<'re> Deserializer<'re> {
    pub fn new(input: &'re str) -> Self {
        Self {
            input,
            curr_key: None,
            is_key: false,
        }
    }

    fn skip(&mut self, n: usize) -> Result<(), Error> {
        if self.input.len() <= n {
            return Err(Error::Eof(self.curr_key.take().unwrap_or_else(|| String::from("unknown"))))
        }

        self.input = &self.input[n..];
        Ok(())
    }

    fn peek_char(&mut self) -> Result<char, Error> {
        self.input.chars().next().ok_or(Error::Eof(self.curr_key.take().unwrap_or_else(|| String::from("unknown"))))
    }

    fn next_char(&mut self) -> Result<char, Error> {
        let ch = self.peek_char()?;
        self.input = &self.input[ch.len_utf8()..];
        Ok(ch)
    }

    ///
    /// If `is_key` is set to true it will not require quotation marks.
    fn parse_string(&mut self) -> Result<&str, Error> {
        if self.peek_char()? == '"' {
            let first_quote = self.next_char()?;
            if first_quote != '"' {
                return Err(Error::ExpectedSymbol {
                    found: first_quote, 
                    expected: '"',
                    at: self.curr_key.take().unwrap_or_else(|| String::from("unknown"))
                })
            }
            
            match self.input.find('"') {
                Some(len) => {
                    let s = &self.input[..len];
                    self.input = &self.input[len + 1..];
                    Ok(s)
                }
                None => Err(Error::Eof(self.curr_key.take().unwrap_or_else(|| String::from("unknown"))))
            }
        } else if self.is_key {
            // continue until colon
            match self.input.find(':') {
                Some(len) => {
                    let s = &self.input[..len];
                    self.input = &self.input[len..];
                    Ok(s)
                },
                None => Err(Error::Eof(self.curr_key.take().unwrap_or_else(|| String::from("unknown"))))
            }
        } else {
            Err(Error::ExpectedSymbol { 
                found: self.peek_char()?, 
                expected: '"',
                at: self.curr_key.take().unwrap_or_else(|| String::from("unknown")) 
            })
        }
    }

    /// Attempts to find the type of number, i.e. byte, short, int, etc... and also parses it.
    /// 
    /// Returns an error if the integer is out of range for the given number. Set `desired` to some
    /// field type if the integer should be of that type.
    fn parse_number<'de, V>(
        &mut self, visitor: V, desired: Option<FieldType>
    ) -> Result<V::Value, Error> where V: Visitor<'de> {
        let negative = self.peek_char()? == '-';
        if negative {
            self.skip(1)?;
        }

        let mut int = match self.peek_char()? {
            ch @ '0'..='9' => {
                (ch as u8 - b'0') as i64
            },
            _ => {
                return Err(Error::ExpectedInteger(self.curr_key.take().unwrap_or_else(|| String::from("unknown"))))
            }
        };

        println!("int is {int}");

        while let ch @ '0'..='9' = self.peek_char()? {
            int
                .checked_mul(10)
                .ok_or_else(|| Error::IntegerTooLarge {
                value: self.input.to_owned(),
                ty: FieldType::Long,
                at: self.curr_key.take().unwrap_or_else(|| String::from("unknown"))
            })?;

            int
                .checked_add((ch as u8 - b'0') as i64)
                .ok_or_else(|| Error::IntegerTooLarge {
                value: self.input.to_owned(),
                ty: FieldType::Long,
                at: self.curr_key.take().unwrap_or_else(|| String::from("unknown"))
            })?;

            self.skip(1)?;
        };

        // Negate if necessary
        if negative {
            int *= -1;
        }

        // Check integer type
        let ty = self.peek_char()?.to_ascii_lowercase();

        let num = match ty {
            'b' => {
                let byte = i8::try_from(int)
                    .map_err(|_| {
                        Error::IntegerTooLarge {
                            value: self.input.to_owned(),
                            ty: FieldType::Byte,
                            at: self.curr_key.take().unwrap_or_else(|| String::from("unknown"))
                        }
                    })?;

                // Verify this is the correct type
                if let Some(ty) = desired {
                    if ty != FieldType::Byte {
                        return Err(Error::UnexpectedType {
                            actual: FieldType::Byte,
                            expected: ty,
                            at: self.curr_key.take().unwrap_or_else(|| String::from("unknown"))
                        })
                    }   
                }       

                self.skip(1)?;
                visitor.visit_i8(byte)
            },
            's' => {
                let short = i16::try_from(int)
                    .map_err(|_| {
                        Error::IntegerTooLarge {
                            value: self.input.to_owned(),
                            ty: FieldType::Short,
                            at: self.curr_key.take().unwrap_or_else(|| String::from("unknown"))
                        }
                    })?;

                    
                // Verify this is the correct type
                if let Some(ty) = desired {
                    if ty != FieldType::Short {
                        return Err(Error::UnexpectedType {
                            actual: FieldType::Short,
                            expected: ty,
                            at: self.curr_key.take().unwrap_or_else(|| String::from("unknown"))
                        })
                    }   
                }

                self.skip(1)?;
                visitor.visit_i16(short)
            },
            // Ints don't have a suffix, so I just try to match for end characters here
            ',' | ']' | '}' => {
                let int = i32::try_from(int)
                    .map_err(|_| {
                        Error::IntegerTooLarge {
                            value: self.input.to_owned(),
                            ty: FieldType::Int,
                            at: self.curr_key.take().unwrap_or_else(|| String::from("unknown"))
                        }
                    })?;

                    
                // Verify this is the correct type
                if let Some(ty) = desired {
                    if ty != FieldType::Int {
                        return Err(Error::UnexpectedType {
                            actual: FieldType::Int,
                            expected: ty,
                            at: self.curr_key.take().unwrap_or_else(|| String::from("unknown"))
                        })
                    }   
                }

                visitor.visit_i32(int)
            },
            'l' => {
                // Verify this is the correct type
                if let Some(ty) = desired {
                    if ty != FieldType::Long {
                        return Err(Error::UnexpectedType {
                            actual: FieldType::Long,
                            expected: ty,
                            at: self.curr_key.take().unwrap_or_else(|| String::from("unknown"))
                        })
                    }   
                }

                self.skip(1)?;
                visitor.visit_i64(int as i64)
            },
            c => {
                println!("end char is {c}");
                todo!();
            }
        };

        num
    }
}

impl<'de> de::Deserializer<'de> for &mut Deserializer<'_> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        if self.is_key {
            return self.deserialize_string(visitor)
        }

        match self.peek_char()? {
            '{' => self.deserialize_map(visitor),
            '[' => self.deserialize_seq(visitor),
            '0'..='9' => self.parse_number(visitor, None),
            '"' => self.deserialize_string(visitor),
            other => {
                println!("Encountered {other}");
                println!("next: {} {}", self.next_char()?, self.next_char()?);
                todo!()
            }
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        let byte = self
            .next_char()?
            .to_digit(10)
            .ok_or_else(|| {
                Error::ExpectedInteger(self.curr_key.take().unwrap_or_else(|| String::from("unknown")))
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
        Err(Error::Unsupported {
            op: "deserializing `char` is not supported",
            at: self.curr_key.take().unwrap_or_else(|| String::from("unknown"))
        })
    }

    fn deserialize_str<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported {
            op: "deserializing string references is not supported",
            at: self.curr_key.take().unwrap_or_else(|| String::from("unknown"))
        })
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

    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_byte_buf<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
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
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
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
    first: bool
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
        K: de::DeserializeSeed<'de>
    {
        let first_char = self.de.next_char()?;
        if self.first && first_char != '{' {
            return Err(Error::ExpectedSymbol {
                found: first_char, 
                expected: '{',
                at: self.de.curr_key.take().unwrap_or_else(|| String::from("unknown"))
            })
        } else if first_char == '}' {
            // Map finished
            return Ok(None)
        } else if !self.first && first_char != ',' {
            return Err(Error::ExpectedSymbol {
                found: first_char, 
                expected: ',',
                at: self.de.curr_key.take().unwrap_or_else(|| String::from("unknown"))
            })
        }

        // Check if map is empty
        if self.de.peek_char()? == '}' {
            return Ok(None)
        }

        self.de.is_key = true;
        let key = seed.deserialize(&mut *self.de).map(Some);
        self.de.is_key = false;
        self.first = false;

        let colon = self.de.next_char()?;
        if colon != ':' {
            return Err(Error::ExpectedSymbol {
                found: colon,
                expected: ':',
                at: self.de.curr_key.take().unwrap_or_else(|| String::from("unknown"))
            })
        }

        key
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Error>
    where
        V: de::DeserializeSeed<'de>
    {
        seed.deserialize(&mut *self.de)   
    }
}

struct ArrayDeserializer<'a, 're> {
    de: &'a mut Deserializer<'re>,
    first: bool
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
        T: de::DeserializeSeed<'de> 
    {
        let first_char = self.de.next_char()?;
        if self.first && first_char != '[' {
            return Err(Error::ExpectedSymbol {
                found: first_char, 
                expected: '[',
                at: self.de.curr_key.take().unwrap_or_else(|| String::from("unknown"))
            })
        } else if first_char == ']' {
            return Ok(None)
        } else if !self.first && first_char != ',' {
            return Err(Error::ExpectedSymbol {
                found: first_char, 
                expected: ',',
                at: self.de.curr_key.take().unwrap_or_else(|| String::from("unknown"))
            })
        }

        // Check whether the array is empty
        if self.de.peek_char()? == ']' {
            return Ok(None)
        }

        self.first = false;

        seed.deserialize(&mut *self.de).map(Some)
    }   
}