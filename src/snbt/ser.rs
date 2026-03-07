use serde::{
    Serialize,
    ser::{self, Impossible, SerializeSeq},
};
use std::any::TypeId;

use crate::Error;

macro_rules! forward_unsupported {
    ($($ty: ident),+) => {
        paste::paste! {$(
            fn [<serialize_ $ty>](self, _v: $ty) -> Result<(), Error> {
                Err(Error::Unsupported(concat!(
                    "serialization of `", stringify!($ty), "` is not supported"
                )))
            }
        )+}
    }
}

#[allow(dead_code)]
pub struct Serializer {
    curr_ty: TypeId,
    is_key: bool,
    pub output: String,
}

impl Serializer {
    pub fn new() -> Serializer {
        Serializer {
            curr_ty: TypeId::of::<()>(),
            is_key: false,
            output: String::new(),
        }
    }
}

impl ser::Serializer for &mut Serializer {
    type Ok = ();

    type Error = Error;

    type SerializeSeq = Self;
    type SerializeTuple = Impossible<(), Error>;
    type SerializeTupleStruct = Impossible<(), Error>;
    type SerializeTupleVariant = Impossible<(), Error>;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Impossible<(), Error>;

    forward_unsupported!(char, u8, u16, u32, u64, u128, i128);

    fn serialize_bool(self, v: bool) -> Result<(), Error> {
        self.output += if v { "true" } else { "false" };
        Ok(())
    }

    fn serialize_i8(self, v: i8) -> Result<(), Error> {
        self.output += &v.to_string();
        self.output += "B";
        Ok(())
    }

    fn serialize_i16(self, v: i16) -> Result<(), Error> {
        self.output += &v.to_string();
        self.output += "S";
        Ok(())
    }

    fn serialize_i32(self, v: i32) -> Result<(), Error> {
        self.output += &v.to_string();
        Ok(())
    }

    fn serialize_i64(self, v: i64) -> Result<(), Error> {
        self.output += &v.to_string();
        self.output += "L";
        Ok(())
    }

    fn serialize_f32(self, v: f32) -> Result<(), Error> {
        self.output += &v.to_string();
        self.output += "F";
        Ok(())
    }

    fn serialize_f64(self, v: f64) -> Result<(), Error> {
        self.output += &v.to_string();
        self.output += "D";
        Ok(())
    }

    fn serialize_str(self, v: &str) -> Result<(), Error> {
        println!("{v}");
        if !self.is_key || v.contains(' ') {
            self.output.reserve(2 + v.len());
            self.output.push('"');
            self.output.push_str(v);
            self.output.push('"');
        } else {
            self.output.reserve(v.len());
            self.output.push_str(v);
        }
        Ok(())
    }

    fn serialize_some<T>(self, v: &T) -> Result<(), Error>
    where
        T: ?Sized + Serialize,
    {
        v.serialize(self)
    }

    fn serialize_none(self) -> Result<(), Error> {
        self.serialize_unit()
    }

    fn serialize_bytes(self, v: &[u8]) -> std::result::Result<Self::Ok, Self::Error> {
        let mut seq = self.serialize_seq(Some(v.len()))?;
        for byte in v {
            seq.serialize_element(byte)?;
        }
        seq.end()
    }

    fn serialize_unit(self) -> std::result::Result<Self::Ok, Self::Error> {
        // Does nothing.
        Ok(())
    }

    fn serialize_unit_struct(
        self,
        _name: &'static str,
    ) -> std::result::Result<Self::Ok, Self::Error> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> std::result::Result<Self::Ok, Self::Error> {
        self.serialize_str(variant)
    }

    fn serialize_newtype_struct<T>(
        self,
        _name: &'static str,
        value: &T,
    ) -> std::result::Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> std::result::Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        todo!()
    }

    fn serialize_seq(
        self,
        _len: Option<usize>,
    ) -> std::result::Result<Self::SerializeSeq, Self::Error> {
        self.output.push('[');
        Ok(self)
    }

    fn serialize_tuple(self, len: usize) -> std::result::Result<Self::SerializeTuple, Self::Error> {
        self.serialize_seq(Some(len))?;
        todo!()
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> std::result::Result<Self::SerializeTupleStruct, Self::Error> {
        todo!()
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> std::result::Result<Self::SerializeTupleVariant, Self::Error> {
        todo!()
    }

    fn serialize_map(
        self,
        _len: Option<usize>,
    ) -> std::result::Result<Self::SerializeMap, Self::Error> {
        self.output.push('{');
        Ok(self)
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> std::result::Result<Self::SerializeStruct, Self::Error> {
        self.serialize_map(Some(len))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> std::result::Result<Self::SerializeStructVariant, Self::Error> {
        todo!()
    }
}

impl ser::SerializeSeq for &mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, v: &T) -> Result<(), Error>
    where
        T: ?Sized + Serialize,
    {
        if !self.output.ends_with('[') {
            self.output.push(',');
        }

        v.serialize(&mut **self)
    }

    fn end(self) -> Result<(), Error> {
        self.output.push(']');
        Ok(())
    }
}

impl ser::SerializeMap for &mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T>(&mut self, k: &T) -> Result<(), Error>
    where
        T: ?Sized + Serialize,
    {
        if !self.output.ends_with('{') {
            self.output.push(',');
        }

        self.is_key = true;
        let result = k.serialize(&mut **self);
        self.is_key = false;
        result
    }

    fn serialize_value<T>(&mut self, v: &T) -> Result<(), Error>
    where
        T: ?Sized + Serialize,
    {
        self.output.push(':');
        v.serialize(&mut **self)
    }

    fn end(self) -> Result<(), Error> {
        self.output.push('}');
        Ok(())
    }
}

impl ser::SerializeStruct for &mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, k: &'static str, v: &T) -> Result<(), Error>
    where
        T: ?Sized + Serialize,
    {
        if !self.output.ends_with("{") {
            self.output.push(',');
        }

        self.is_key = true;
        k.serialize(&mut **self)?;
        self.is_key = false;
        self.output.push(':');
        v.serialize(&mut **self)
    }

    fn end(self) -> Result<(), Error> {
        self.output.push('}');
        Ok(())
    }
}

impl ser::SerializeStructVariant for &mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _k: &'static str, _v: &T) -> Result<(), Error>
    where
        T: ?Sized + Serialize,
    {
        todo!()
    }

    fn end(self) -> Result<(), Error> {
        todo!()
    }
}
