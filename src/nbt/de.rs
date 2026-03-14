use std::marker::PhantomData;

use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use paste::paste;
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, de};
use varint_rs::VarintReader;

use crate::error::{UnexpectedEnd, UnexpectedEof, UnexpectedType, Unsupported};
use crate::{EndiannessImpl, Error, FieldType, NetworkLittleEndian, Variant};

/// Verifies that the deserialized type is equal to the expected type.
macro_rules! is_ty {
    ($expected: ident, $field_name: expr, $actual: expr) => {
        if $actual != FieldType::$expected {
            return Err(Error::UnexpectedType(UnexpectedType {
                expected: FieldType::$expected,
                actual: $actual,

                #[cfg(feature = "error-context")]
                at: $field_name
                    .take()
                    .unwrap_or_else(|| String::from("unknown")),
                #[cfg(feature = "error-context")]
                index: None,
            }));
        }
    };
}

/// Returns a `not supported` error.
macro_rules! forward_unsupported {
    ($($ty: ident),+) => {
        paste! {$(

            fn [<deserialize_ $ty>]<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
            where
                V: Visitor<'de>
            {
                Err(Error::Unsupported(Unsupported {
                    op: concat!("deserialization of `", stringify!($ty), "` is not supported"),
                    #[cfg(feature = "error-context")]
                    at: self.curr_key.take().unwrap_or_else(|| String::from("unknown")),
                    #[cfg(feature = "error-context")]
                    index: None
                }))
            }
        )+}
    }
}

/// NBT deserializer. Rather than using this directly you should probably use one of the methods
/// provided in the root, such as [`from_le_bytes`].
#[derive(Debug)]
pub struct Deserializer<'re, 'de, F, R>
where
    R: ReadBytesExt,
    F: EndiannessImpl + 'de,
{
    input: &'re mut R,
    next_ty: FieldType,
    is_key: bool,
    #[cfg(feature = "error-context")]
    curr_key: Option<String>,
    _marker: PhantomData<&'de F>,
}

impl<'re, 'de, F, R> Deserializer<'re, 'de, F, R>
where
    R: ReadBytesExt,
    F: EndiannessImpl + 'de,
{
    /// Creates a new deserializer, consuming the reader.
    pub fn new(input: &'re mut R) -> Result<Self, Error> {
        let next_ty = FieldType::try_from(
            input.read_u8()?, 
            #[cfg(feature = "error-context")]
            &mut None, 
            #[cfg(feature = "error-context")]
            Some(0)
        )?;
        if next_ty != FieldType::Compound {
            return Err(Error::UnexpectedType(UnexpectedType {
                actual: next_ty,
                expected: FieldType::Compound,
                #[cfg(feature = "error-context")]
                at: String::from("root"),
                #[cfg(feature = "error-context")]
                index: Some(0),
            }));
        }

        let de = Deserializer {
            input,
            next_ty,
            is_key: false,
            #[cfg(feature = "error-context")]
            curr_key: None,
            _marker: PhantomData,
        };

        // Ignore name of root component
        let len = match F::AS_ENUM {
            Variant::BigEndian => de.input.read_u16::<BigEndian>()? as u32,
            Variant::LittleEndian => de.input.read_u16::<LittleEndian>()? as u32,
            Variant::NetworkEndian => de.input.read_u32_varint()?,
        };

        let mut buf = vec![0; len as usize];
        de.input.read_exact(&mut buf)?;

        let _name = String::from_utf8(buf)?;

        Ok(de)
    }
}

/// Reads a single object of type `T` from the given buffer.
///
/// On success, the deserialized object and number of bytes read from the buffer are returned.
pub fn from_bytes<'de, 're, F, T>(reader: &'re mut impl ReadBytesExt) -> Result<T, Error>
where
    T: Deserialize<'de>,
    F: EndiannessImpl + 'de,
{
    let mut deserializer = Deserializer::<F, _>::new(reader)?;
    let output = T::deserialize(&mut deserializer)?;

    Ok(output)
}

/// Reads a single object of type `T` from the given buffer.
///
/// This function uses the little endian format of NBT, which is used by disk formats
/// in Minecraft: Bedrock Edition.
///
/// On success, the deserialised object and amount of bytes read from the buffer are returned.
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
/// # let data = Data {
/// #   value: String::from("Hello, World!")
/// # };
/// # let obuffer = nbtx::to_le_bytes(&data)?;
/// # let mut buffer: &[u8] = obuffer.as_ref();
///
///  let data: Data = nbtx::from_le_bytes(&mut buffer)?;
///
///  println!("Got {data:?}!");
/// # Ok(())
/// # }
/// ```
pub fn from_le_bytes<'de, T, R>(reader: &mut R) -> Result<T, Error>
where
    R: ReadBytesExt,
    T: Deserialize<'de>,
{
    from_bytes::<LittleEndian, T>(reader)
}

/// Reads a single object of type `T` from the given buffer.
///
/// This function uses the little endian format of NBT, which is used by
/// Minecraft: Java Edition.
///
/// On success, the deserialised object and amount of bytes read from the buffer are returned.
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
/// # let data = Data {
/// #   value: String::from("Hello, World!")
/// # };
/// # let owned_buffer = nbtx::to_be_bytes(&data)?;
/// # let mut buffer = owned_buffer.as_slice();
///
///  let data: Data = nbtx::from_be_bytes(&mut buffer)?;
///
///  println!("Got {data:?}!");
/// # Ok(())
/// # }
/// ```
pub fn from_be_bytes<'de, T, R>(reader: &mut R) -> Result<T, Error>
where
    R: ReadBytesExt,
    T: Deserialize<'de>,
{
    from_bytes::<BigEndian, T>(reader)
}

/// Reads a single object of type `T` from the given buffer.
///
/// This function uses the variable format of NBT, which is used by network formats
/// in Minecraft: Bedrock Edition.
///
/// On success, the deserialised object and amount of bytes read from the buffer are returned.
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
/// # let data = Data {
/// #   value: String::from("Hello, World!")
/// # };
/// # let owned_buffer = nbtx::to_net_bytes(&data)?;
/// # let mut buffer = owned_buffer.as_slice();
///
///  let data: Data = nbtx::from_net_bytes(&mut buffer)?;
///  println!("Got {data:?}!");
/// # Ok(())
/// # }
/// ```
pub fn from_net_bytes<'data, T, R>(reader: &mut R) -> Result<T, Error>
where
    R: ReadBytesExt,
    T: Deserialize<'data>,
{
    from_bytes::<NetworkLittleEndian, T>(reader)
}

impl<'de, 'a, F, R> de::Deserializer<'de> for &'a mut Deserializer<'_, 'de, F, R>
where
    R: ReadBytesExt,
    F: EndiannessImpl + 'a,
{
    type Error = Error;

    forward_unsupported!(char, u8, u16, u32, u64, i128, u128);

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        if self.is_key {
            self.deserialize_string(visitor)
        } else {
            match self.next_ty {
                FieldType::End => Err(Error::UnexpectedEnd(UnexpectedEnd {
                    #[cfg(feature = "error-context")]
                    at: self
                        .curr_key
                        .take()
                        .unwrap_or_else(|| String::from("unknown")),
                    #[cfg(feature = "error-context")]
                    index: None,
                })),
                FieldType::Byte => self.deserialize_i8(visitor),
                FieldType::Short => self.deserialize_i16(visitor),
                FieldType::Int => self.deserialize_i32(visitor),
                FieldType::Long => self.deserialize_i64(visitor),
                FieldType::Float => self.deserialize_f32(visitor),
                FieldType::Double => self.deserialize_f64(visitor),
                FieldType::ByteArray => self.deserialize_seq(visitor),
                FieldType::String => self.deserialize_string(visitor),
                FieldType::List => self.deserialize_seq(visitor),
                FieldType::Compound => self.deserialize_map(visitor),
                FieldType::IntArray => self.deserialize_seq(visitor),
                FieldType::LongArray => self.deserialize_seq(visitor),
            }
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        is_ty!(Byte, self.curr_key, self.next_ty);

        let n = self.input.read_u8()? != 0;
        visitor.visit_bool(n)
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        is_ty!(Byte, self.curr_key, self.next_ty);

        let n = self.input.read_i8()?;
        visitor.visit_i8(n)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        is_ty!(Short, self.curr_key, self.next_ty);

        let n = match F::AS_ENUM {
            Variant::BigEndian => self.input.read_i16::<BigEndian>(),
            Variant::LittleEndian | Variant::NetworkEndian => self.input.read_i16::<LittleEndian>(),
        }?;

        visitor.visit_i16(n)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        is_ty!(Int, self.curr_key, self.next_ty);

        let n = match F::AS_ENUM {
            Variant::BigEndian => self.input.read_i32::<BigEndian>(),
            Variant::LittleEndian => self.input.read_i32::<LittleEndian>(),
            Variant::NetworkEndian => self.input.read_i32_varint(),
        }?;

        visitor.visit_i32(n)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        is_ty!(Long, self.curr_key, self.next_ty);

        let n = match F::AS_ENUM {
            Variant::BigEndian => self.input.read_i64::<BigEndian>(),
            Variant::LittleEndian => self.input.read_i64::<LittleEndian>(),
            Variant::NetworkEndian => self.input.read_i64_varint(),
        }?;

        visitor.visit_i64(n)
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        is_ty!(Float, self.curr_key, self.next_ty);

        let n = match F::AS_ENUM {
            Variant::BigEndian => self.input.read_f32::<BigEndian>(),
            _ => self.input.read_f32::<LittleEndian>(),
        }?;

        visitor.visit_f32(n)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        is_ty!(Double, self.curr_key, self.next_ty);

        let n = match F::AS_ENUM {
            Variant::BigEndian => self.input.read_f64::<BigEndian>(),
            _ => self.input.read_f64::<LittleEndian>(),
        }?;

        visitor.visit_f64(n)
    }

    fn deserialize_str<V>(self, _visitor: V) -> Result<V::Value, Error>
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
            index: None,
        }))
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        is_ty!(String, self.curr_key, self.next_ty);

        let len = match F::AS_ENUM {
            Variant::BigEndian => self.input.read_u16::<BigEndian>()? as u32,
            Variant::LittleEndian => self.input.read_u16::<LittleEndian>()? as u32,
            Variant::NetworkEndian => self.input.read_u32_varint()?,
        };

        let mut buf = vec![0; len as usize];
        self.input.read_exact(&mut buf)?;

        let string = String::from_utf8(buf)?;

        #[cfg(feature = "error-context")]
        if self.is_key {
            self.curr_key = Some(string.clone());
        }

        visitor.visit_string(string)
    }

    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported(Unsupported {
            op: "deserializing byte slices is not supported",
            #[cfg(feature = "error-context")]
            at: self
                .curr_key
                .take()
                .unwrap_or_else(|| String::from("unknown")),
            #[cfg(feature = "error-context")]
            index: None,
        }))

        // is_ty!(ByteArray, self.field_name, self.next_ty);

        // let len = match F::AS_ENUM {
        //     Variant::BigEndian => self.input.read_i32::<BigEndian>()? as u32,
        //     Variant::LittleEndian => self.input.read_i32::<LittleEndian>()? as u32,
        //     Variant::NetworkEndian => self.input.read_i32_varint()? as u32,
        // };

        // // let mut buf = Vec::with_capacity(len as usize);
        // // self.input.read_exact(&mut buf)?;
        // //
        // todo!("Obtain slice from cursor directly without copying to heap");

        // // visitor.visit_bytes(&buf)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        is_ty!(ByteArray, self.curr_key, self.next_ty);

        let len = match F::AS_ENUM {
            Variant::BigEndian => self.input.read_i32::<BigEndian>()? as u32,
            Variant::LittleEndian => self.input.read_i32::<LittleEndian>()? as u32,
            Variant::NetworkEndian => self.input.read_i32_varint()? as u32,
        };

        let mut buf = vec![0; len as usize];
        self.input.read_exact(&mut buf)?;

        visitor.visit_byte_buf(buf)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        // This is only used to represent possibly missing fields.
        // If this code is reached, it means the key was found and the field exists.
        // Therefore this is always some.
        visitor.visit_some(self)
    }

    fn deserialize_unit<V>(self, _visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported(Unsupported {
            op: "deserializing unit values is not supported",
            #[cfg(feature = "error-context")]
            at: self
                .curr_key
                .take()
                .unwrap_or_else(|| String::from("unknown")),
            #[cfg(feature = "error-context")]                
            index: None,
        }))
    }

    fn deserialize_unit_struct<V>(self, _name: &'static str, _visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported(Unsupported {
            op: "deserializing unit structs is not supported",
            #[cfg(feature = "error-context")]
            at: self
                .curr_key
                .take()
                .unwrap_or_else(|| String::from("unknown")),
            #[cfg(feature = "error-context")]                
            index: None,
        }))
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        _visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported(Unsupported {
            op: "deserializing newtype structs is not supported",
            #[cfg(feature = "error-context")]
            at: self
                .curr_key
                .take()
                .unwrap_or_else(|| String::from("unknown")),
            #[cfg(feature = "error-context")]                
            index: None,
        }))
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_tuple(0, visitor)
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        let ty = match self.next_ty {
            FieldType::ByteArray => FieldType::Byte,
            FieldType::IntArray => FieldType::Int,
            FieldType::LongArray => FieldType::Long,
            _ => FieldType::try_from(
                self.input.read_u8()?, 
                #[cfg(feature = "error-context")]
                &mut self.curr_key, 
                #[cfg(feature = "error-context")]
                None
            )?,
        };

        let de = SeqDeserializer::new(self, ty, len as u32)?;
        visitor.visit_seq(de)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        _visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported(Unsupported {
            op: "deserializing tuple structs is not supported",
            #[cfg(feature = "error-context")]
            at: self
                .curr_key
                .take()
                .unwrap_or_else(|| String::from("unknown")),
            #[cfg(feature = "error-context")]                
            index: None,
        }))
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        is_ty!(Compound, self.curr_key, self.next_ty);

        let de = MapDeserializer::from(self);
        visitor.visit_map(de)
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported(Unsupported {
            op: "deserializing enums is not supported",
            #[cfg(feature = "error-context")]
            at: self
                .curr_key
                .take()
                .unwrap_or_else(|| String::from("unknown")),
            #[cfg(feature = "error-context")]                  
            index: None,
        }))
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}

/// Deserializes NBT sequences.
///
/// Sequences are in this case: [`ByteArray`](FieldType::ByteArray), [`IntArray`](FieldType::IntArray)
/// [`LongArray`](FieldType::LongArray) and [`List`](FieldType::List).
#[derive(Debug)]
struct SeqDeserializer<'a, 're, 'de: 'a, F, R>
where
    R: ReadBytesExt,
    F: EndiannessImpl,
{
    de: &'a mut Deserializer<'re, 'de, F, R>,
    ty: FieldType,
    remaining: u32,
}

impl<'de, 're, 'a, F, R> SeqDeserializer<'a, 're, 'de, F, R>
where
    R: ReadBytesExt,
    F: EndiannessImpl,
{
    pub fn new(
        de: &'a mut Deserializer<'re, 'de, F, R>,
        ty: FieldType,
        expected_len: u32,
    ) -> Result<Self, Error> {
        // debug_assert_ne!(ty, FieldType::End, "Cannot serialize sequence of end tags");

        // ty is not read in here because the x_array types don't have a type prefix.

        de.next_ty = ty;
        let remaining = match F::AS_ENUM {
            Variant::BigEndian => de.input.read_i32::<BigEndian>()? as u32,
            Variant::LittleEndian => de.input.read_i32::<LittleEndian>()? as u32,
            Variant::NetworkEndian => de.input.read_i32_varint()? as u32,
        };

        if expected_len != 0 && expected_len != remaining {
            return Err(Error::UnexpectedEof(UnexpectedEof {
                #[cfg(feature = "error-context")]
                at: de
                    .curr_key
                    .take()
                    .unwrap_or_else(|| String::from("unknown")),
                #[cfg(feature = "error-context")]
                index: None,
            }));
        }

        Ok(Self { de, ty, remaining })
    }
}

impl<'de, F, R> SeqAccess<'de> for SeqDeserializer<'_, '_, 'de, F, R>
where
    R: ReadBytesExt,
    F: EndiannessImpl,
{
    type Error = Error;

    fn next_element_seed<E>(&mut self, seed: E) -> Result<Option<E::Value>, Error>
    where
        E: DeserializeSeed<'de>,
    {
        if self.remaining > 0 {
            self.remaining -= 1;

            let output = seed.deserialize(&mut *self.de).map(Some);
            self.de.next_ty = self.ty;
            output
        } else {
            Ok(None)
        }
    }
}

/// Deserialises NBT compounds.
#[derive(Debug)]
struct MapDeserializer<'a, 're, 'de: 'a, F, R>
where
    R: ReadBytesExt,
    F: EndiannessImpl,
{
    de: &'a mut Deserializer<'re, 'de, F, R>,
}

impl<'de, 're, 'a, F, R> From<&'a mut Deserializer<'re, 'de, F, R>>
    for MapDeserializer<'a, 're, 'de, F, R>
where
    R: ReadBytesExt,
    F: EndiannessImpl,
{
    fn from(v: &'a mut Deserializer<'re, 'de, F, R>) -> Self {
        Self { de: v }
    }
}

impl<'de, F, R> MapAccess<'de> for MapDeserializer<'_, '_, 'de, F, R>
where
    R: ReadBytesExt,
    F: EndiannessImpl,
{
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Error>
    where
        K: DeserializeSeed<'de>,
    {
        self.de.is_key = true;
        self.de.next_ty = FieldType::String;

        let next_ty = FieldType::try_from(
            self.de.input.read_u8()?, 
            #[cfg(feature = "error-context")]
            &mut self.de.curr_key, 
            #[cfg(feature = "error-context")]
            None
        )?;

        let r = if next_ty == FieldType::End {
            Ok(None)
        } else {
            seed.deserialize(&mut *self.de).map(Some)
        };

        self.de.is_key = false;
        self.de.next_ty = next_ty;
        r
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Error>
    where
        V: DeserializeSeed<'de>,
    {
        debug_assert_ne!(
            self.de.next_ty,
            FieldType::End,
            "Cannot serialize end as a map field"
        );
        seed.deserialize(&mut *self.de)
    }
}
