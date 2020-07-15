//! Stratum V2 binary (de)serializers with Serde

use std::convert::TryFrom;
use std::convert::TryInto;
use std::fmt;
use std::io;
use std::marker::PhantomData;
use std::result::Result as StdResult;
use std::slice;

use serde;
use serde::de::Deserializer as _;
use serde::de::{DeserializeSeed, EnumAccess, IntoDeserializer, SeqAccess, VariantAccess};
use serde::ser::Impossible;
use serde::{de, ser, Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Sequence too long")]
    Overlong,
    #[error("Sequence too short")]
    TooShort,
    #[error("Sequence length unknown ahead of time")]
    UnknownLength,
    #[error("Type `{0}` unsupported by the protocol")]
    Unsupported(&'static str),
    #[error("Invalid Unicode string/character data")]
    Unicode,
    #[error("Incomplete message, unexpected end of input data")]
    EOF,
    #[error("Trailing data left after deserialization")]
    TrailingBytes,
    #[error("Found value other than 1 or 0 when deserializing a bool")]
    BadBool,
    #[error("{0}")]
    Custom(String),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
}

pub type Result<T> = StdResult<T, Error>;

impl ser::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Error::Custom(format!("{}", msg))
    }
}

impl de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Error::Custom(format!("{}", msg))
    }
}

// Serialization

#[derive(Debug)]
struct Serializer<W> {
    writer: W,
}

impl<W: io::Write> Serializer<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    fn write(&mut self, buf: &[u8]) -> Result<()> {
        self.writer.write_all(buf).map_err(From::from)
    }
}

impl<'a, W: io::Write> ser::Serializer for &'a mut Serializer<W> {
    type Ok = ();
    type Error = Error;

    type SerializeSeq = SizedSeqEmitter<'a, W, u16>;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Impossible<(), Error>;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, v: bool) -> Result<()> {
        let byte = if v { &[1u8] } else { &[0u8] };
        self.write(byte)
    }

    fn serialize_i8(self, v: i8) -> Result<()> {
        self.write(&[v as u8])
    }

    fn serialize_i16(self, v: i16) -> Result<()> {
        self.write(&v.to_le_bytes())
    }

    fn serialize_i32(self, v: i32) -> Result<()> {
        self.write(&v.to_le_bytes())
    }

    fn serialize_i64(self, v: i64) -> Result<()> {
        self.write(&v.to_le_bytes())
    }

    fn serialize_u8(self, v: u8) -> Result<()> {
        self.write(&v.to_le_bytes())
    }

    fn serialize_u16(self, v: u16) -> Result<()> {
        self.write(&v.to_le_bytes())
    }

    fn serialize_u32(self, v: u32) -> Result<()> {
        self.write(&v.to_le_bytes())
    }

    fn serialize_u64(self, v: u64) -> Result<()> {
        self.write(&v.to_le_bytes())
    }

    fn serialize_f32(self, v: f32) -> Result<()> {
        self.write(&v.to_bits().to_le_bytes())
    }

    fn serialize_f64(self, v: f64) -> Result<()> {
        self.write(&v.to_bits().to_le_bytes())
    }

    fn serialize_char(self, v: char) -> Result<()> {
        self.serialize_u32(v as u32)
    }

    fn serialize_str(self, v: &str) -> Result<()> {
        let len: u16 = v.len().try_into().map_err(|_| Error::Overlong)?;
        self.serialize_u16(len)?;
        self.write(v.as_bytes())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        let len: u16 = v.len().try_into().map_err(|_| Error::Overlong)?;
        self.serialize_u16(len)?;
        self.write(v)
    }

    fn serialize_none(self) -> Result<()> {
        self.serialize_bool(false)
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<()> {
        self.serialize_bool(true)?;
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<()> {
        Ok(())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        Ok(())
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
    ) -> Result<()> {
        variant_index.serialize(self)
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<()> {
        match name {
            "Str1_32" => value.serialize(SizedSeqEmitter::<W, u8>::new(self)),
            "Str0_32" => value.serialize(SizedSeqEmitter::<W, u8>::new(self)),
            "Str1_255" => value.serialize(SizedSeqEmitter::<W, u8>::new(self)),
            "Str0_255" => value.serialize(SizedSeqEmitter::<W, u8>::new(self)),

            "Bytes0_32" => value.serialize(SizedSeqEmitter::<W, u8>::new(self)),
            "Bytes1_32" => value.serialize(SizedSeqEmitter::<W, u8>::new(self)),
            "Bytes0_255" => value.serialize(SizedSeqEmitter::<W, u8>::new(self)),
            "Bytes1_255" => value.serialize(SizedSeqEmitter::<W, u8>::new(self)),
            "Bytes0_64k" => value.serialize(SizedSeqEmitter::<W, u16>::new(self)),
            "Bytes1_64k" => value.serialize(SizedSeqEmitter::<W, u16>::new(self)),

            "Seq0_255" => value.serialize(SizedSeqEmitter::<W, u8>::new(self)),
            "Seq0_64k" => value.serialize(SizedSeqEmitter::<W, u16>::new(self)),

            _ => value.serialize(self),
        }
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        value: &T,
    ) -> Result<()> {
        variant_index.serialize(&mut *self)?;
        value.serialize(self)
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq> {
        let emitter = SizedSeqEmitter::new(self);
        emitter.serialize_seq(len)
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        Ok(self)
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        Ok(self)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        variant_index.serialize(&mut *self)?;
        Ok(self)
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        Err(Error::Unsupported("Map"))
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        variant_index.serialize(&mut *self)?;
        Ok(self)
    }
}

impl<'a, W: io::Write> ser::SerializeSeq for &'a mut Serializer<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl<'a, W: io::Write> ser::SerializeTuple for &'a mut Serializer<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl<'a, W: io::Write> ser::SerializeTupleStruct for &'a mut Serializer<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl<'a, W: io::Write> ser::SerializeTupleVariant for &'a mut Serializer<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl<'a, W: io::Write> ser::SerializeStruct for &'a mut Serializer<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        _key: &'static str,
        value: &T,
    ) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl<'a, W: io::Write> ser::SerializeStructVariant for &'a mut Serializer<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        _key: &'static str,
        value: &T,
    ) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

struct SizedSeqEmitter<'a, W, I> {
    serializer: &'a mut Serializer<W>,
    _marker: PhantomData<*const I>,
}

impl<'a, W, I> SizedSeqEmitter<'a, W, I> {
    fn new(serializer: &'a mut Serializer<W>) -> Self {
        Self {
            serializer,
            _marker: PhantomData,
        }
    }
}

impl<'a, W, I> ser::Serializer for SizedSeqEmitter<'a, W, I>
where
    W: io::Write,
    I: TryFrom<usize> + Serialize,
{
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Self;
    type SerializeTuple = Impossible<(), Error>;
    type SerializeTupleStruct = Impossible<(), Error>;
    type SerializeTupleVariant = Impossible<(), Error>;
    type SerializeMap = Impossible<(), Error>;
    type SerializeStruct = Impossible<(), Error>;
    type SerializeStructVariant = Impossible<(), Error>;

    fn serialize_bool(self, _v: bool) -> Result<()> {
        unreachable!()
    }

    fn serialize_i8(self, _v: i8) -> Result<()> {
        unreachable!()
    }

    fn serialize_i16(self, _v: i16) -> Result<()> {
        unreachable!()
    }

    fn serialize_i32(self, _v: i32) -> Result<()> {
        unreachable!()
    }

    fn serialize_i64(self, _v: i64) -> Result<()> {
        unreachable!()
    }

    fn serialize_u8(self, _v: u8) -> Result<()> {
        unreachable!()
    }

    fn serialize_u16(self, _v: u16) -> Result<()> {
        unreachable!()
    }

    fn serialize_u32(self, _v: u32) -> Result<()> {
        unreachable!()
    }

    fn serialize_u64(self, _v: u64) -> Result<()> {
        unreachable!()
    }

    fn serialize_f32(self, _v: f32) -> Result<()> {
        unreachable!()
    }

    fn serialize_f64(self, _v: f64) -> Result<()> {
        unreachable!()
    }

    fn serialize_char(self, _v: char) -> Result<()> {
        unreachable!()
    }

    fn serialize_str(self, value: &str) -> Result<()> {
        let len = I::try_from(value.len()).map_err(|_| Error::Overlong)?;
        len.serialize(&mut *self.serializer)?;
        self.serializer.write(value.as_bytes())
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<()> {
        unreachable!()
    }

    fn serialize_none(self) -> Result<()> {
        unreachable!()
    }

    fn serialize_some<T: ?Sized + Serialize>(self, _value: &T) -> Result<()> {
        unreachable!()
    }

    fn serialize_unit(self) -> Result<()> {
        unreachable!()
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        unreachable!()
    }

    fn serialize_unit_variant(self, _: &'static str, _: u32, _: &'static str) -> Result<()> {
        unreachable!()
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(self, _: &'static str, _: &T) -> Result<()> {
        unreachable!()
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<()> {
        unreachable!()
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq> {
        let len = len.ok_or(Error::UnknownLength)?;
        let len = I::try_from(len).map_err(|_| Error::Overlong)?;
        len.serialize(&mut *self.serializer)?;
        Ok(self)
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        unreachable!()
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        unreachable!()
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        unreachable!()
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        unreachable!()
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        unreachable!()
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        unreachable!()
    }
}

impl<'a, W, I> ser::SerializeSeq for SizedSeqEmitter<'a, W, I>
where
    W: io::Write,
    I: TryFrom<usize> + Serialize,
{
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.serializer)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

#[inline]
pub fn to_writer<W, T>(writer: W, value: &T) -> Result<()>
where
    W: io::Write,
    T: ?Sized + Serialize,
{
    let mut ser = Serializer::new(writer);
    value.serialize(&mut ser)
}

pub fn to_vec<T: ?Sized + Serialize>(value: &T) -> Result<Vec<u8>> {
    // TODO: Performance: fine-tune this to some typical message size
    let mut buffer = Vec::with_capacity(128);
    to_writer(&mut buffer, value)?;
    Ok(buffer)
}

// Deserialization

pub struct Deserializer<'de> {
    input: slice::Iter<'de, u8>,
}

struct FwdSeqAccess<'a, 'de> {
    deserializer: &'a mut Deserializer<'de>,
    len: usize,
}

impl<'a, 'de> FwdSeqAccess<'a, 'de> {
    fn new(deserializer: &'a mut Deserializer<'de>, len: usize) -> Self {
        Self { deserializer, len }
    }
}

impl<'a, 'de> SeqAccess<'de> for FwdSeqAccess<'a, 'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: DeserializeSeed<'de>,
    {
        if self.len > 0 {
            self.len -= 1;
            let value = DeserializeSeed::deserialize(seed, &mut *self.deserializer)?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.len)
    }
}

impl<'de> Deserializer<'de> {
    pub fn from_slice(input: &'de [u8]) -> Self {
        Deserializer {
            input: input.iter(),
        }
    }

    // TODO: rewrite these when const generics are (more) stable

    #[inline]
    fn read_u8(&mut self) -> Result<u8> {
        self.input.next().map(|x| *x).ok_or(Error::EOF)
    }

    #[inline]
    fn read_u16(&mut self) -> Result<u16> {
        let bytes = self.read_bytes(2)?;
        let bytes: [u8; 2] = bytes.try_into().expect("BUG: Invalid slice size");
        Ok(u16::from_le_bytes(bytes))
    }

    #[inline]
    fn read_u32(&mut self) -> Result<u32> {
        let bytes = self.read_bytes(4)?;
        let bytes: [u8; 4] = bytes.try_into().expect("BUG: Invalid slice size");
        Ok(u32::from_le_bytes(bytes))
    }

    #[inline]
    fn read_u64(&mut self) -> Result<u64> {
        let bytes = self.read_bytes(8)?;
        let bytes: [u8; 8] = bytes.try_into().expect("BUG: Invalid slice size");
        Ok(u64::from_le_bytes(bytes))
    }

    #[inline]
    fn read_bytes(&mut self, size: usize) -> Result<&'de [u8]> {
        let res = self.input.as_slice().get(..size).ok_or(Error::EOF)?;
        if size > 0 {
            let _ = self.input.nth(size - 1);
        }
        Ok(res)
    }

    #[inline]
    fn read_str(&mut self, size: usize) -> Result<&'de str> {
        let slice = self.read_bytes(size)?;
        std::str::from_utf8(&slice).map_err(|_| Error::Unicode)
    }

    #[inline]
    fn deserialize_sized_seq<S, V>(
        &mut self,
        min_size: usize,
        max_size: usize,
        size_read_fn: fn(&mut Self) -> Result<S>,
        visitor: V,
    ) -> Result<V::Value>
    where
        S: TryInto<usize>,
        V: de::Visitor<'de>,
    {
        let size = size_read_fn(self)?
            .try_into()
            .map_err(|_| Error::Overlong)?;

        match size {
            size if size < min_size => Err(Error::TooShort),
            size if size > max_size => Err(Error::Overlong),
            size => visitor.visit_newtype_struct(SizedSeqDeserializer::new(self, size)),
        }
    }

    fn read_bool(&mut self) -> Result<bool> {
        let byte = self.read_u8()?;
        match byte {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::BadBool),
        }
    }

    #[inline]
    fn bytes_left(&self) -> usize {
        self.input.as_slice().len()
    }
}

impl<'de, 'a> de::Deserializer<'de> for &'a mut Deserializer<'de> {
    type Error = Error;

    fn deserialize_any<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        Err(Error::Unsupported("Any / Dynamic"))
    }

    fn deserialize_bool<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let b = self.read_bool()?;
        visitor.visit_bool(b)
    }

    fn deserialize_i8<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let num = self.read_u8()?;
        visitor.visit_i8(num as i8)
    }

    fn deserialize_i16<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let num = self.read_u16()?;
        visitor.visit_i16(num as i16)
    }

    fn deserialize_i32<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let num = self.read_u32()?;
        visitor.visit_i32(num as i32)
    }

    fn deserialize_i64<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let num = self.read_u64()?;
        visitor.visit_i64(num as i64)
    }

    fn deserialize_u8<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let num = self.read_u8()?;
        visitor.visit_u8(num)
    }

    fn deserialize_u16<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let num = self.read_u16()?;
        visitor.visit_u16(num)
    }

    fn deserialize_u32<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let num = self.read_u32()?;
        visitor.visit_u32(num)
    }

    fn deserialize_u64<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let num = self.read_u64()?;
        visitor.visit_u64(num)
    }

    fn deserialize_f32<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let bits = self.read_u32()?;
        visitor.visit_f32(f32::from_bits(bits))
    }

    fn deserialize_f64<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let bits = self.read_u64()?;
        visitor.visit_f64(f64::from_bits(bits))
    }

    fn deserialize_char<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let c = self.read_u32()?;
        let c = std::char::from_u32(c).ok_or(Error::Unicode)?;
        visitor.visit_char(c)
    }

    fn deserialize_str<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let size = self.read_u16()? as usize;
        let s = self.read_str(size)?;
        visitor.visit_borrowed_str(s)
    }

    fn deserialize_string<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let size = self.read_u16()? as usize;
        let s = self.read_str(size)?;
        visitor.visit_str(s)
    }

    fn deserialize_bytes<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let size = self.read_u16()? as usize;
        let bytes = self.read_bytes(size)?;
        visitor.visit_borrowed_bytes(bytes)
    }

    fn deserialize_byte_buf<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let size = self.read_u16()? as usize;
        let bytes = self.read_bytes(size)?;
        visitor.visit_bytes(bytes.into())
    }

    fn deserialize_option<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let flag = self.read_bool()?;
        if flag {
            visitor.visit_some(self)
        } else {
            visitor.visit_none()
        }
    }

    fn deserialize_unit<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V: de::Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        match name {
            "Str0_32" => self.deserialize_sized_seq(0, 32, Deserializer::read_u8, visitor),
            "Str1_32" => self.deserialize_sized_seq(1, 32, Deserializer::read_u8, visitor),
            "Str0_255" => self.deserialize_sized_seq(0, 255, Deserializer::read_u8, visitor),
            "Str1_255" => self.deserialize_sized_seq(1, 255, Deserializer::read_u8, visitor),

            "Bytes0_32" => self.deserialize_sized_seq(0, 32, Deserializer::read_u8, visitor),
            "Bytes1_32" => self.deserialize_sized_seq(1, 32, Deserializer::read_u8, visitor),
            "Bytes0_255" => self.deserialize_sized_seq(0, 255, Deserializer::read_u8, visitor),
            "Bytes1_255" => self.deserialize_sized_seq(1, 255, Deserializer::read_u8, visitor),
            "Bytes0_64k" => self.deserialize_sized_seq(0, 65535, Deserializer::read_u16, visitor),
            "Bytes1_64k" => self.deserialize_sized_seq(1, 65535, Deserializer::read_u16, visitor),

            "Seq0_255" => self.deserialize_sized_seq(0, 255, Deserializer::read_u8, visitor),
            "Seq0_64k" => self.deserialize_sized_seq(0, 65535, Deserializer::read_u16, visitor),

            _ => visitor.visit_newtype_struct(self),
        }
    }

    fn deserialize_seq<V: de::Visitor<'de>>(mut self, visitor: V) -> Result<V::Value> {
        let size: usize = self.read_u16()?.try_into().map_err(|_| Error::Overlong)?;

        visitor.visit_seq(FwdSeqAccess::new(&mut self, size))
    }

    fn deserialize_tuple<V: de::Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value> {
        struct Access<'a, 'de> {
            deserializer: &'a mut Deserializer<'de>,
            len: usize,
        }

        impl<'a, 'de> SeqAccess<'de> for Access<'a, 'de> {
            type Error = Error;

            fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
            where
                T: DeserializeSeed<'de>,
            {
                if self.len > 0 {
                    self.len -= 1;
                    let value = DeserializeSeed::deserialize(seed, &mut *self.deserializer)?;
                    Ok(Some(value))
                } else {
                    Ok(None)
                }
            }

            fn size_hint(&self) -> Option<usize> {
                Some(self.len)
            }
        }

        visitor.visit_seq(Access {
            deserializer: self,
            len: len,
        })
    }

    fn deserialize_tuple_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_tuple(len, visitor)
    }

    fn deserialize_map<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        Err(Error::Unsupported("Map"))
    }

    fn deserialize_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_tuple(fields.len(), visitor)
    }

    fn deserialize_enum<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_enum(self)
    }

    fn deserialize_identifier<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_u32(visitor)
    }

    fn deserialize_ignored_any<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        Err(Error::Unsupported("Any / Dynamic type"))
    }
}

impl<'a, 'de> EnumAccess<'de> for &'a mut Deserializer<'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant)>
    where
        V: DeserializeSeed<'de>,
    {
        let idx: u32 = Deserialize::deserialize(&mut *self)?;
        let val: Result<_> = seed.deserialize(idx.into_deserializer());
        Ok((val?, self))
    }
}

impl<'a, 'de> VariantAccess<'de> for &'a mut Deserializer<'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        Ok(())
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value>
    where
        T: DeserializeSeed<'de>,
    {
        DeserializeSeed::deserialize(seed, self)
    }

    fn tuple_variant<V>(self, len: usize, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_tuple(len, visitor)
    }

    fn struct_variant<V>(self, fields: &'static [&'static str], visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_tuple(fields.len(), visitor)
    }
}

struct SizedSeqDeserializer<'a, 'de> {
    deserializer: &'a mut Deserializer<'de>,
    size: usize,
}

impl<'a, 'de> SizedSeqDeserializer<'a, 'de> {
    fn new(deserializer: &'a mut Deserializer<'de>, size: usize) -> Self {
        Self { deserializer, size }
    }
}

impl<'a, 'de> de::Deserializer<'de> for SizedSeqDeserializer<'a, 'de> {
    type Error = Error;

    fn deserialize_any<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_bool<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_i8<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_i16<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_i32<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_i64<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_u8<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_u16<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_u32<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_u64<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_f32<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_f64<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_char<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_str<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_string<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let s = self.deserializer.read_str(self.size)?;
        visitor.visit_string(s.into())
    }

    fn deserialize_bytes<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_byte_buf<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_option<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_unit<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_unit_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _visitor: V,
    ) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_newtype_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_seq(FwdSeqAccess::new(self.deserializer, self.size))
    }

    fn deserialize_tuple<V: de::Visitor<'de>>(self, _len: usize, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_tuple_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        _visitor: V,
    ) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_map<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_enum<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_identifier<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }

    fn deserialize_ignored_any<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unreachable!()
    }
}

pub fn from_slice<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T> {
    let mut deserializer = Deserializer::from_slice(bytes);
    let value = T::deserialize(&mut deserializer)?;
    if deserializer.bytes_left() == 0 {
        Ok(value)
    } else {
        Err(Error::TrailingBytes)
    }
}

// Tests

#[cfg(test)]
mod test {
    use std::convert::TryInto;
    use std::iter;

    use super::*;
    use crate::v2::types::*;

    #[derive(PartialEq, Serialize, Deserialize, Default, Clone, Debug)]
    struct SeqItem(u32, f32);

    impl SeqItem {
        fn make_seq() -> Vec<Self> {
            vec![SeqItem(0, 0.0), SeqItem(1, 0.25)]
        }
    }

    #[test]
    fn v2_serialize_numerals() {
        let bytes = to_vec(&123u32).expect("BUG: serialization failed");
        assert_eq!(&bytes, &[123, 0, 0, 0]);

        let bytes = to_vec(&1.0f32).expect("BUG: serialization failed");
        assert_eq!(&bytes, &[0, 0, 0x80, 0x3f]);
    }

    #[test]
    fn v2_serialize_string() {
        let s = String::from("abc");
        let bytes = to_vec(&s).expect("BUG: Serialization failure");
        assert_eq!(&bytes, &[3, 0, 0x61, 0x62, 0x63]);

        let s: Str1_32 = "abc".try_into().expect("BUG: Str1_32 constructor failure");
        let bytes = to_vec(&s).expect("BUG: Serialization failure");
        assert_eq!(&bytes, &[3, 0x61, 0x62, 0x63]);

        let s: Str1_255 = "abc".try_into().expect("BUG: Str1_255 constructor failure");
        let bytes = to_vec(&s).expect("BUG: Serialization failure");
        assert_eq!(&bytes, &[3, 0x61, 0x62, 0x63]);

        // Zero-sized string
        let s = Str0_32::new();
        let bytes = to_vec(&s).expect("BUG: Serialization failure");
        assert_eq!(&bytes, &[0]);

        // Overlong strings
        let s_long: String = iter::repeat('_').take(256).collect();
        Str1_32::try_from(&s_long[..32]).expect("BUG: Str1_32 constructor failure");
        Str1_32::try_from(&s_long[..33])
            .err()
            .expect("BUG: Str1_32 constructor didn't fail but should have");

        Str1_255::try_from(&s_long[..255]).expect("BUG: Str1_255 constructor failure");
        Str1_255::try_from(s_long)
            .err()
            .expect("BUG: Str1_255 constructor didn't fail but should have");

        let s_long: String = iter::repeat('_').take(1 << 16).collect();
        match to_vec(&s_long) {
            Err(Error::Overlong) => {}
            Err(err) => panic!(
                "Deserialization failed with unexpected error value: {:?}",
                err
            ),
            Ok(_) => panic!("Serialization didn't fail but should have"),
        }
    }

    #[test]
    fn v2_deserialize_string() {
        let bytes = [3, 0, 0x61, 0x62, 0x63];
        let s: &str = from_slice(&bytes).expect("BUG: Deserialization failure");
        assert_eq!(s, "abc");

        let bytes = [3, 0, 0x61, 0x62, 0x63];
        let s: String = from_slice(&bytes).expect("BUG: Deserialization failure");
        assert_eq!(s, "abc");

        let bytes = [3, 0x61, 0x62, 0x63];
        let s: Str1_32 = from_slice(&bytes).expect("BUG: Deserialization failure");
        assert_eq!(s.as_str(), "abc");

        let s: Str1_255 = from_slice(&bytes).expect("BUG: Deserialization failure");
        assert_eq!(s.as_str(), "abc");

        // Zero-sized string
        let bytes = [0];
        let s: Str0_255 = from_slice(&bytes).expect("BUG: Deserialization failure");
        assert_eq!(s.as_str(), "");

        // Overlong string
        let bytes = [33];
        match from_slice::<Str1_32>(&bytes) {
            Err(Error::Overlong) => {}
            Err(err) => panic!(
                "Deserialization failed with unexpected error value: {:?}",
                err
            ),
            Ok(_) => panic!("Deserialization didn't fail but should have"),
        }

        // Unexpected zero size
        let bytes = [0];
        match from_slice::<Str1_255>(&bytes) {
            Err(Error::TooShort) => {}
            Err(err) => panic!(
                "Deserialization failed with unexpected error value: {:?}",
                err
            ),
            Ok(_) => panic!("Deserialization didn't fail but should have"),
        }
    }

    #[test]
    fn v2_serialize_bytes() {
        let bytes = vec![1u8, 2, 3];
        let bytes = to_vec(&bytes).expect("BUG: Serialization failure");
        assert_eq!(&bytes, &[3, 0, 1, 2, 3]);

        let bytes: Bytes0_32 = vec![1, 2, 3]
            .try_into()
            .expect("BUG: Bytes0_32 constructor failure");
        let bytes = to_vec(&bytes).expect("BUG: Serialization failure");
        assert_eq!(&bytes, &[3, 1, 2, 3]);

        let bytes: Bytes1_64k = vec![1, 2, 3]
            .try_into()
            .expect("BUG: Bytes1_64k constructor failure");
        let bytes = to_vec(&bytes).expect("BUG: Serialization failure");
        assert_eq!(&bytes, &[3, 0, 1, 2, 3]);

        // Zero-sized byte buffer
        let bytes = Bytes0_255::new();
        let bytes = to_vec(&bytes).expect("BUG: Serialization failure");
        assert_eq!(&bytes, &[0]);

        // Overlong buffer
        let bytes: Vec<u8> = iter::repeat(1).take(256).collect();
        Bytes1_32::try_from(&bytes[..32]).expect("BUG: Bytes1_32 constructor failure");
        Bytes1_32::try_from(&bytes[..33])
            .err()
            .expect("BUG: Bytes1_32 constructor didn't fail but should have");

        // Large buffer
        let bytes: Vec<u8> = iter::repeat(1).take(64 * 1024 - 1).collect();
        let bytes: Bytes0_64k = bytes
            .try_into()
            .expect("BUG: Bytes1_64k constructor failure");
        let bytes = to_vec(&bytes).expect("BUG: Serialization failure");
        assert_eq!(&bytes[..2], &[0xff, 0xff]);
    }

    #[test]
    fn v2_deserialize_bytes() {
        let bytes = [3, 0, 1, 2, 3];
        let bytes: &[u8] = from_slice(&bytes).expect("BUG: Deserialization failure");
        assert_eq!(bytes, &[1, 2, 3]);

        let bytes = [3, 0, 1, 2, 3];
        let bytes: Vec<u8> = from_slice(&bytes).expect("BUG: Deserialization failure");
        assert_eq!(bytes, &[1, 2, 3]);

        let bytes = [3, 1, 2, 3];
        let bytes: Bytes0_32 = from_slice(&bytes).expect("BUG: Deserialization failure");
        assert_eq!(&*bytes, &[1, 2, 3]);

        let bytes = [3, 0, 1, 2, 3];
        let bytes: Bytes1_64k = from_slice(&bytes).expect("BUG: Deserialization failure");
        assert_eq!(&*bytes, &[1, 2, 3]);

        // Zero-sized buffer
        let bytes = [0];
        let s: Bytes0_255 = from_slice(&bytes).expect("BUG: Deserialization failure");
        assert_eq!(s.len(), 0);

        // Overlong buffer
        let bytes = [33];
        match from_slice::<Bytes1_32>(&bytes) {
            Err(Error::Overlong) => {}
            Err(err) => panic!(
                "Deserialization failed with unexpected error value: {:?}",
                err
            ),
            Ok(_) => panic!("Deserialization didn't fail but should have"),
        }

        // Unexpected zero size
        let bytes = [0];
        match from_slice::<Bytes1_255>(&bytes) {
            Err(Error::TooShort) => {}
            Err(err) => panic!(
                "Deserialization failed with unexpected error value: {:?}",
                err
            ),
            Ok(_) => panic!("Deserialization didn't fail but should have"),
        }
    }

    #[rustfmt::skip]
    static SEQ_BIN_255: &'static [u8] = &[
        2u8,
        0, 0, 0, 0,
        0, 0, 0, 0,
        1, 0, 0, 0,
        0, 0, 128, 62
    ];

    #[rustfmt::skip]
    static SEQ_BIN_64K: &'static [u8] = &[
        2u8, 0,
        0, 0, 0, 0,
        0, 0, 0, 0,
        1, 0, 0, 0,
        0, 0, 128, 62
    ];

    #[rustfmt::skip]
    static SEQ_BIN_64K_BYTES_64K: &'static [u8] = &[
        2u8, 0,
        3u8, 0,
        1, 2, 3,
        3u8, 0,
        4, 5, 6,
    ];

    #[test]
    fn v2_serialize_seq() {
        let seq: Seq0_255<SeqItem> = SeqItem::make_seq()
            .try_into()
            .expect("BUG: Seq0_255 constructor failure");
        let bytes = to_vec(&seq).expect("BUG: Serialization failure");
        assert_eq!(&bytes, &SEQ_BIN_255);

        let seq: Seq0_64k<SeqItem> = SeqItem::make_seq()
            .try_into()
            .expect("BUG: Seq0_64k constructor failure");
        let bytes = to_vec(&seq).expect("BUG: Serialization failure");
        assert_eq!(&bytes, &SEQ_BIN_64K);

        // Overlong seq
        let seq: Vec<SeqItem> = iter::repeat(SeqItem::default()).take(256).collect();
        Seq0_255::try_from(seq.as_slice())
            .err()
            .expect("BUG: Seq0_255 constructor didn't fail but should have");

        let seq: Vec<SeqItem> = iter::repeat(SeqItem::default()).take(1 << 16).collect();
        Seq0_64k::try_from(seq.as_slice())
            .err()
            .expect("BUG: Seq0_64k constructor didn't fail but should have");
    }

    #[test]
    fn v2_serialize_nested_bytes_in_seq() {
        let seq: Seq0_64k<Bytes0_64k> = vec![
            vec![1, 2, 3].try_into().expect("BUG: cannot convert"),
            vec![4, 5, 6].try_into().expect("BUG: cannot convert"),
        ]
        .try_into()
        .expect("BUG: cannot convert vec to Seq");
        let bytes = to_vec(&seq).expect("BUG: serialization failure");
        assert_eq!(&bytes, &SEQ_BIN_64K_BYTES_64K);
    }

    #[test]
    fn v2_deserialize_seq() {
        let seq: Seq0_255<SeqItem> =
            from_slice(&SEQ_BIN_255).expect("BUG: Deserialization failure");
        let expected: Seq0_255<SeqItem> = SeqItem::make_seq()
            .try_into()
            .expect("BUG: Seq0_255 constructor failure");
        assert_eq!(&seq, &expected);

        let seq: Seq0_64k<SeqItem> =
            from_slice(&SEQ_BIN_64K).expect("BUG: Deserialization failure");
        let expected: Seq0_64k<SeqItem> = SeqItem::make_seq()
            .try_into()
            .expect("BUG: Seq0_64k constructor failure");
        assert_eq!(&seq, &expected);

        // EOF
        let bytes = [1u8];
        match from_slice::<Seq0_255<SeqItem>>(&bytes) {
            Err(Error::EOF) => {}
            Err(err) => panic!(
                "Deserialization failed with unexpected error value: {:?}",
                err
            ),
            Ok(_) => panic!("Deserialization didn't fail but should have"),
        }
    }

    #[test]
    fn v2_serialization_roundtrip() {
        #[derive(PartialEq, Serialize, Deserialize, Debug)]
        enum MyEnum {
            Unit,
            Tuple(f32),
            Struct { data: f64 },
        }

        #[derive(PartialEq, Serialize, Deserialize, Debug)]
        struct MyData<'a> {
            b: bool,
            num_u8: u8,
            num_i8: i8,
            num_u16: u16,
            num_i16: i16,
            num_u32: u32,
            num_i32: i32,
            num_u64: u64,
            num_i64: i64,
            s_32: Str1_32,
            s_255: Str1_255,
            e_unit: MyEnum,
            e_tuple: MyEnum,
            e_struct: MyEnum,
            vec: Vec<u8>,
            seq_255: Seq0_255<SeqItem>,
            seq_64k: Seq0_64k<SeqItem>,
            slice: &'a [u8],
        }

        let bytes = vec![0, 1, 2, 3];

        let my_data = MyData {
            b: true,
            num_u8: 1,
            num_i8: -1,
            num_u16: 2,
            num_i16: -2,
            num_u32: 3,
            num_i32: -3,
            num_u64: 4,
            num_i64: -4,
            s_32: Str1_32::try_from("Hello, World!").expect("BUG: Str1_32 c-tor failed"),
            s_255: Str1_255::try_from("Hello, World!").expect("BUG: Str1_255 c-tor failed"),
            e_unit: MyEnum::Unit,
            e_tuple: MyEnum::Tuple(3.14),
            e_struct: MyEnum::Struct { data: 1.618 },
            vec: bytes.clone(),
            seq_255: SeqItem::make_seq()
                .try_into()
                .expect("BUG: cannot build SEQ0_255"),
            seq_64k: SeqItem::make_seq()
                .try_into()
                .expect("BUG: cannot build SEQ0_64k"),
            slice: bytes.as_slice(),
        };

        let bytes = to_vec(&my_data).expect("BUG: Serialization failed");

        let my_data_2: MyData = from_slice(&bytes).expect("BUG: Deserialization failed");
        assert_eq!(my_data, my_data_2);
    }
}
