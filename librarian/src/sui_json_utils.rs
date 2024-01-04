
use move_core_types::{
    language_storage::StructTag,
    value::{MoveStruct, MoveValue}
};
use serde_json::{
    json,
    Value as JsonValue
};
use std::collections::BTreeMap;
use sui_sdk::types::{
    base_types::{
        SuiAddress,
        STD_ASCII_MODULE_NAME, STD_ASCII_STRUCT_NAME, STD_OPTION_MODULE_NAME,
        STD_OPTION_STRUCT_NAME, STD_UTF8_MODULE_NAME, STD_UTF8_STRUCT_NAME,
    },
    id::ID,
    MOVE_STDLIB_ADDRESS,
};

pub fn move_value_to_json(move_value: &MoveValue) -> Option<JsonValue> {
    Some(match move_value {
        MoveValue::Vector(values) => JsonValue::Array(
            values
                .iter()
                .map(move_value_to_json)
                .collect::<Option<_>>()?,
        ),
        MoveValue::Bool(v) => json!(v),
        MoveValue::Signer(v) | MoveValue::Address(v) => json!(SuiAddress::from(*v).to_string()),
        MoveValue::U8(v) => json!(v),
        MoveValue::U64(v) => json!(v.to_string()),
        MoveValue::U128(v) => json!(v.to_string()),
        MoveValue::U16(v) => json!(v),
        MoveValue::U32(v) => json!(v),
        MoveValue::U256(v) => json!(v.to_string()),
        MoveValue::Struct(move_struct) => match move_struct {
            MoveStruct::Runtime(values) => {
                let values = values.iter().map(move_value_to_json).collect::<Vec<_>>();
                json!(values)
            }
            MoveStruct::WithTypes { fields, type_ } if is_move_string_type(type_) => {
                // ascii::string and utf8::string has a single bytes field.
                let (_, v) = fields.first()?;
                let string: String = bcs::from_bytes(&v.simple_serialize()?).ok()?;
                json!(string)
            }
            MoveStruct::WithTypes { fields, type_ } if is_move_option_type(type_) => {
                // option has a single vec field.
                let (_, v) = fields.first()?;
                if let MoveValue::Vector(v) = v {
                    JsonValue::Array(v.iter().filter_map(move_value_to_json).collect::<Vec<_>>())
                } else {
                    return None;
                }
            }
            MoveStruct::WithTypes { fields, type_ } if type_ == &ID::type_() => {
                // option has a single vec field.
                let (_, v) = fields.first()?;
                if let MoveValue::Address(address) = v {
                    json!(SuiAddress::from(*address))
                } else {
                    return None;
                }
            }
            // We only care about values here, assuming struct type information is known at the client side.
            MoveStruct::WithTypes { fields, .. } | MoveStruct::WithFields(fields) => {
                let fields = fields
                    .iter()
                    .map(|(key, value)| (key, move_value_to_json(value)))
                    .collect::<BTreeMap<_, _>>();
                json!(fields)
            }
        },
    })
}

fn is_move_string_type(tag: &StructTag) -> bool {
    (tag.address == MOVE_STDLIB_ADDRESS
        && tag.module.as_ident_str() == STD_UTF8_MODULE_NAME
        && tag.name.as_ident_str() == STD_UTF8_STRUCT_NAME)
        || (tag.address == MOVE_STDLIB_ADDRESS
            && tag.module.as_ident_str() == STD_ASCII_MODULE_NAME
            && tag.name.as_ident_str() == STD_ASCII_STRUCT_NAME)
}

fn is_move_option_type(tag: &StructTag) -> bool {
    tag.address == MOVE_STDLIB_ADDRESS
        && tag.module.as_ident_str() == STD_OPTION_MODULE_NAME
        && tag.name.as_ident_str() == STD_OPTION_STRUCT_NAME
}