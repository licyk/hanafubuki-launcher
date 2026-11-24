//! The field names a spec struct declares, read from serde itself.
//!
//! `#[derive(Deserialize)]` hands a struct's serialized field names to
//! `Deserializer::deserialize_struct`. Reading them there lets persistence,
//! the install catalog, and the Agent contract test ask a family's spec which
//! fields it accepts, instead of each keeping its own copy of the list.

use serde::de::{self, DeserializeOwned, Visitor};
use serde::forward_to_deserialize_any;

struct FieldNames<'a> {
    fields: &'a mut &'static [&'static str],
}

impl<'de> de::Deserializer<'de> for FieldNames<'_> {
    type Error = de::value::Error;

    fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Self::Error> {
        Err(de::Error::custom("field names are read from a struct only"))
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        *self.fields = fields;
        Err(de::Error::custom("field names read"))
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes
        byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map enum
        identifier ignored_any
    }
}

pub(crate) fn declared_fields<T: DeserializeOwned>() -> &'static [&'static str] {
    let mut fields: &'static [&'static str] = &[];
    let _ = T::deserialize(FieldNames {
        fields: &mut fields,
    });
    fields
}
