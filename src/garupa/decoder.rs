//! Small lenient protobuf wire decoder used for the profile response subset.

use std::collections::HashMap;

use serde_json::{Map, Value};

use super::schema::{ProtoType, Schema};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WireType {
    Varint,
    Fixed32,
    Fixed64,
    LengthDelimited,
}

#[derive(Debug, Clone)]
enum RawData {
    Varint(u64),
    Bytes(Vec<u8>),
}

#[derive(Debug, Clone)]
struct RawField {
    field: u32,
    wire_type: WireType,
    data: RawData,
}

#[derive(Debug)]
pub struct ProtoError(pub String);

impl std::fmt::Display for ProtoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn read_varint(buf: &[u8], offset: usize) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut shift = 0u32;
    let mut cursor = offset;

    while cursor < buf.len() {
        let byte = buf[cursor];
        cursor += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some((value, cursor));
        }
        shift += 7;
        if shift > 56 {
            return None;
        }
    }

    None
}

fn parse_raw_fields(buf: &[u8]) -> Vec<RawField> {
    let mut fields = Vec::new();
    let mut offset = 0;

    while offset < buf.len() {
        let (key, next) = match read_varint(buf, offset) {
            Some(value) => value,
            None => break,
        };
        offset = next;
        let field = (key >> 3) as u32;
        if field == 0 {
            break;
        }

        match key & 0x07 {
            0 => {
                let (value, next) = match read_varint(buf, offset) {
                    Some(value) => value,
                    None => break,
                };
                offset = next;
                fields.push(RawField {
                    field,
                    wire_type: WireType::Varint,
                    data: RawData::Varint(value),
                });
            }
            1 => {
                if offset + 8 > buf.len() {
                    break;
                }
                fields.push(RawField {
                    field,
                    wire_type: WireType::Fixed64,
                    data: RawData::Bytes(buf[offset..offset + 8].to_vec()),
                });
                offset += 8;
            }
            2 => {
                let (length, next) = match read_varint(buf, offset) {
                    Some(value) => value,
                    None => break,
                };
                offset = next;
                let length = length as usize;
                if offset + length > buf.len() {
                    break;
                }
                fields.push(RawField {
                    field,
                    wire_type: WireType::LengthDelimited,
                    data: RawData::Bytes(buf[offset..offset + length].to_vec()),
                });
                offset += length;
            }
            5 => {
                if offset + 4 > buf.len() {
                    break;
                }
                fields.push(RawField {
                    field,
                    wire_type: WireType::Fixed32,
                    data: RawData::Bytes(buf[offset..offset + 4].to_vec()),
                });
                offset += 4;
            }
            _ => break,
        }
    }

    fields
}

pub fn decode(buf: &[u8], schema: &Schema) -> Result<Value, ProtoError> {
    let mut groups: HashMap<u32, Vec<RawField>> = HashMap::new();
    for field in parse_raw_fields(buf) {
        groups.entry(field.field).or_default().push(field);
    }

    let mut result = Map::new();
    for (tag, definition) in schema.fields {
        let Some(items) = groups.get(tag) else {
            continue;
        };

        let parse = |item: &RawField| -> Option<Value> {
            match (definition.ty, &item.wire_type, &item.data) {
                (ProtoType::Int | ProtoType::Long, WireType::Varint, RawData::Varint(value)) => {
                    Some(Value::from(*value as i64))
                }
                (ProtoType::String, WireType::LengthDelimited, RawData::Bytes(value)) => {
                    Some(Value::String(String::from_utf8_lossy(value).into_owned()))
                }
                (ProtoType::Message(sub), WireType::LengthDelimited, RawData::Bytes(value)) => {
                    decode(value, sub).ok()
                }
                _ => None,
            }
        };

        if definition.repeated {
            result.insert(
                definition.name.to_string(),
                Value::Array(items.iter().filter_map(parse).collect()),
            );
        } else if let Some(value) = items.iter().rev().find_map(parse) {
            result.insert(definition.name.to_string(), value);
        }
    }

    Ok(Value::Object(result))
}
