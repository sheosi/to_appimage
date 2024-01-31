use std::{fmt::Display, io::Write};

use serde::{ser, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Custom(String),
}

impl serde::ser::Error for Error {
    fn custom<T>(msg:T) -> Self where T:Display {
        Self::Custom(msg.to_string())
    }
}

type Result<T> = std::result::Result<T, Error>;

struct LevelTracker {
    level: u8,
    key_name: Option<String>
}

impl LevelTracker {
    fn new() -> Self {
        Self { level: 0, key_name: None}
    }

    fn open_level(&mut self) -> u8 {
        self.level += 1;
        self.level
    }

    fn close_level(&mut self) -> u8 {
        if self.level == 2 {
            self.key_name = None;
        }
        let res = self.level;
        self.level -= 1;
        res
    }

    fn get_level(&self) -> u8 {self.level}
    fn get_key(&self) -> &Option<String> {&self.key_name}
    fn set_key(&mut self, key: String) {self.key_name = Some(key);}
}

pub struct Serializer {
    // This string starts empty and JSON is appended as values are serialized.
    output: String,
    level: LevelTracker,
    disable_write_key: bool
}

pub fn to_string<T>(value:&T) -> Result<String>  where T: ?Sized + Serialize{
    let mut serializer = Serializer{output: String ::new(), level: LevelTracker::new(), disable_write_key: false};
    value.serialize(&mut serializer)?;
    Ok(serializer.output)
}

pub fn to_writer<W,T>(mut writer: W, value: &T) -> Result<()> 
where
    W: Write,
    T: ?Sized + Serialize,
{
    writer.write_all(to_string(value)?.as_bytes()).unwrap();
    Ok(())
}

impl Serializer {
    fn write_pre_val(&mut self) {
        if self.level.get_level() == 2 && !self.disable_write_key {
            self.output.push_str(self.level.get_key().as_ref().unwrap());
            self.output.push('=');
        }
    } 
}

impl<'a> ser::Serializer for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, v: bool) -> Result<()> {
        self.write_pre_val();
        self.output += if v { "true" } else { "false" };
        Ok(())
    }

    fn serialize_i8(self, v: i8) ->Result<()> {
        self.serialize_i64(i64::from(v))
    }

    fn serialize_i16(self, v: i16) ->Result<()> {
        self.serialize_i64(i64::from(v))
    }

    fn serialize_i32(self, v: i32) ->Result<()> {
        self.serialize_i64(i64::from(v))
    }

    fn serialize_i64(self, v: i64) ->Result<()> {
        self.write_pre_val();
        self.output += &v.to_string();
        Ok(())
    }

    fn serialize_u8(self, v: u8) -> Result<()> {
        self.serialize_u64(u64::from(v))
    }

    fn serialize_u16(self, v: u16) -> Result<()> {
        self.serialize_u64(u64::from(v))
    }

    fn serialize_u32(self, v: u32) -> Result<()> {
        self.serialize_u64(u64::from(v))
    }

    fn serialize_u64(self, v: u64) -> Result<()> {
        self.write_pre_val();
        self.output += &v.to_string();
        Ok(())
    }


    fn serialize_f32(self, v: f32) -> Result<()> {
        self.serialize_f64(f64::from(v))
    }

    fn serialize_f64(self, v: f64) -> Result<()> {
        self.write_pre_val();
        self.output += &v.to_string();
        Ok(())
    }

    fn serialize_char(self, v: char) -> Result<()> {
        self.serialize_str(&v.to_string())
    }

    // This only works for strings that don't require escape sequences but you
    // get the idea. For example it would emit invalid JSON if the input string
    // contains a '"' character.
    fn serialize_str(self, v: &str) -> Result<()> {
        self.write_pre_val();
        self.output += v;
        Ok(())
    }

    // Serialize a byte array as an array of bytes. Could also use a base64
    // string here. Binary formats will typically represent byte arrays more
    // compactly.
    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        use serde::ser::SerializeSeq;
        self.write_pre_val();
        self.disable_write_key = true;
        let mut seq = self.serialize_seq(Some(v.len()))?;
        for byte in v {
            seq.serialize_element(byte)?;
        }
        seq.end()
    }

    // An absent optional is represented as the JSON `null`.
    fn serialize_none(self) -> Result<()> {
        self.serialize_unit()
    }

    // Unit struct means a named value containing no data. Again, since there is
    // no data, map this to JSON as `null`. There is no need to serialize the
    // name in most formats.
    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        self.serialize_unit()
    }


    fn serialize_some<T>(self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    // In Serde, unit means an anonymous value containing no data. We'll 
    // translate to an empty string.
    fn serialize_unit(self) -> Result<()> {
        self.write_pre_val();
        self.output += "";
        Ok(())
    }

    // When serializing a unit variant (or any other kind of variant), formats
    // can choose whether to keep track of it by index or by name. Binary
    // formats typically use the index of the variant and human-readable formats
    // typically use the name.
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<()> {
        self.serialize_str(variant)
    }
    
    // As is done here, serializers are encouraged to treat newtype structs as
    // insignificant wrappers around the data they contain.
    fn serialize_newtype_struct<T>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    // Note that newtype variant (and all of the other variant serialization
    // methods) refer exclusively to the "externally tagged" enum
    // representation.
    //
    // Serialize this to JSON in externally tagged form as `{ NAME: VALUE }`.
    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.write_pre_val();
        variant.serialize(&mut *self)?;
        self.output += "=";
        value.serialize(&mut *self)?;
        Ok(())
    }


    // Tuple variants are represented in JSON as `{ NAME: [DATA...] }`. Again
    // this method is only responsible for the externally tagged representation.
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        self.write_pre_val();
        self.output += "{";
        variant.serialize(&mut *self)?;
        self.output += ":[";
        Ok(self)
    }

    // Compount types
    //
    // The start of the sequence, each value, and the end are three separate
    // method calls. This one is responsible only for serializing the start,
    // which in JSON is `[`.
    //
    // The length of the sequence may or may not be known ahead of time. This
    // doesn't make a difference in JSON because the length is not represented
    // explicitly in the serialized form. Some serializers may only be able to
    // support sequences for which the length is known up front.
    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        self.write_pre_val();
        self.disable_write_key = true;
        Ok(self)
    }

    // Tuples look just like sequences in JSON. Some formats may be able to
    // represent tuples more efficiently by omitting the length, since tuple
    // means that the corresponding `Deserialize implementation will know the
    // length without needing to look at the serialized data.
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple> {
        self.serialize_seq(Some(len))
    }

    // Tuple structs look just like sequences in JSON.
    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        self.serialize_seq(Some(len))
    }


    // Maps are represented in JSON as `{ K: V, K: V, ... }`.
    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        self.level.open_level();
        Ok(self)
    }

    // Structs look just like maps in JSON. In particular, JSON requires that we
    // serialize the field names of the struct. Other formats may be able to
    // omit the field names when serializing structs because the corresponding
    // Deserialize implementation is required to know what the keys are without
    // looking at the serialized data.
    fn serialize_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct> {
        self.serialize_map(Some(len))
    }

    // Struct variants are represented in JSON as `{ NAME: { K: V, ... } }`.
    // This is the externally tagged representation.
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        self.output += "{";
        variant.serialize(&mut *self)?;
        self.output += ":{";
        Ok(self)
    }
}

// The following 7 impls deal with the serialization of compound types like
// sequences and maps. Serialization of such types is begun by a Serializer
// method and followed by zero or more calls to serialize individual elements of
// the compound type and one call to end the compound type.
//
// This impl is SerializeSeq so these methods are called after `serialize_seq`
// is called on the Serializer.
impl<'a> ser::SerializeSeq for &'a mut Serializer {
    // Must match the `Ok` type of the serializer.
    type Ok = ();
    // Must match the `Error` type of the serializer.
    type Error = Error;

    // Serialize a single element of the sequence.
    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        if !self.output.ends_with('=') {
            self.output += ";";
        }
        value.serialize(&mut **self)
    }

    // Close the sequence.
    fn end(self) -> Result<()> {
        self.output += ";";
        self.disable_write_key = false;
        Ok(())
    }
}

// Same thing but for tuples.
impl<'a> ser::SerializeTuple for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        if !self.output.ends_with('=') {
            self.output += ";";
        }
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        self.output += ";";
        Ok(())
    }
}

// Same thing but for tuple structs.
impl<'a> ser::SerializeTupleStruct for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        if !self.output.ends_with('=') {
            self.output += ";";
        }
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        self.output += ";";
        Ok(())
    }
}

// Tuple variants are a little different. Refer back to the
// `serialize_tuple_variant` method above:
//
//    self.output += "{";
//    variant.serialize(&mut *self)?;
//    self.output += ":[";
//
// So the `end` method in this impl is responsible for closing both the `]` and
// the `}`.
impl<'a> ser::SerializeTupleVariant for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        if !self.output.ends_with('=') {
            self.output += ";";
        }
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        self.output += ";";
        Ok(())
    }
}

// Some `Serialize` types are not able to hold a key and value in memory at the
// same time so `SerializeMap` implementations are required to support
// `serialize_key` and `serialize_value` individually.
//
// There is a third optional method on the `SerializeMap` trait. The
// `serialize_entry` method allows serializers to optimize for the case where
// key and value are both available simultaneously. In JSON it doesn't make a
// difference so the default behavior for `serialize_entry` is fine.
impl<'a> ser::SerializeMap for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    // The Serde data model allows map keys to be any serializable type. JSON
    // only allows string keys so the implementation below will produce invalid
    // JSON if the key serializes as something other than a string.
    //
    // A real JSON serializer would need to validate that map keys are strings.
    // This can be done by using a different Serializer to serialize the key
    // (instead of `&mut **self`) and having that other serializer only
    // implement `serialize_str` and return an error on any other data type.
    fn serialize_key<T>(&mut self, key: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        let mut temp_ser = Serializer{output: String::new(), level: LevelTracker::new(), disable_write_key: true};
        key.serialize(&mut temp_ser)?;
        match self.level.get_level() {
            0 => {panic!("EEEErm")}
            1 => {
                self.output += "[";
                key.serialize(&mut **self)?;
                self.output += "]\n";
            }
            2 => {
                self.level.set_key(temp_ser.output);
            }
            3 => {
                self.output += self.level.get_key().as_ref().unwrap();
                self.output += "[";
                key.serialize(&mut **self)?;
                self.output += "]=";
            }
            l => return Err(Error::Custom(format!("freedesktop entries have a maximum of three levels {l}")))
        }
        Ok(())
    }

    // It doesn't make a difference whether the colon is printed at the end of
    // `serialize_key` or at the beginning of `serialize_value`. In this case
    // the code is a bit simpler having it here.
    fn serialize_value<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(&mut **self)?;
        self.output+="\n";
        Ok(())
    }

    fn end(self) -> Result<()> {
        self.level.close_level();
        Ok(())
    }
}

// Structs are like maps in which the keys are constrained to be compile-time
// constant strings.
impl<'a> ser::SerializeStruct for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {   
        match self.level.get_level() {
            0 => {panic!("EEEErm")},
            1 => {
                self.output += "[";
                key.serialize(&mut **self)?;
                self.output += "]\n";
            },
            2 => {
                self.level.set_key(key.to_string());
            },
            3 => {
                self.output += self.level.get_key().as_ref().unwrap();
                self.output += "[";
                key.serialize(&mut **self)?;
                self.output += "]=";
            },
            l => return Err(Error::Custom(format!("freedesktop entries have a maximum of three levels {l}")))
        }
        value.serialize(&mut **self)?;
        self.output+="\n";
        Ok(())
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

// Similar to `SerializeTupleVariant`, here the `end` method is responsible for
// closing both of the curly braces opened by `serialize_struct_variant`.
impl<'a> ser::SerializeStructVariant for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        key.serialize(&mut **self)?;
        self.output += ":";
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        self.output += "}}";
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::to_string;
    use serde::Serialize;
    
    #[derive(Serialize)]
    struct TestBasic {
        #[serde(rename="Desktop Entry")]
        a: InnerString
    }

    #[derive(Serialize)]
    struct InnerString {
        #[serde(rename="Test")  ]
        b: String,
        c: String
    }
    
    #[test]
    fn basic() {
        assert_eq!(
            &to_string(&TestBasic{a:InnerString { b:"test string".to_string(), c:"Another one".to_string()}}).unwrap(),
            "[Desktop Entry]
Test=test string
c=Another one

"
        );
    }

    #[derive(Serialize)]
    struct TestTranslations {
        #[serde(rename="Desktop Entry")]
        a: InnerTranslations
    }

    #[derive(Serialize)]
    struct InnerTranslations {
        b: HashMap<String, String>
    }

    #[test]
    fn translations() {
        let mut map = HashMap::new();
        map.insert("es".to_string(), "A".to_uppercase());
        map.insert("en".to_string(), "B".to_string());
        assert_eq!(&to_string(&TestTranslations{a:InnerTranslations{b: map}}).unwrap(),
        "[Desktop Entry]
b[es]=A
b[en]=B


"
    );
    }

    #[derive(Serialize)]
    struct TestSeq {
        #[serde(rename="Desktop Entry")]
        a: InnerSeq
    }

    #[derive(Serialize)]
    struct InnerSeq {
        b: Vec<String>
    }

    #[test]
    fn seq() {
        assert_eq!(&to_string(&TestSeq{a:InnerSeq{b: vec!["test".to_string(), "string".to_string()]}}).unwrap(),
        "[Desktop Entry]
b=test;string;

");
    }
}