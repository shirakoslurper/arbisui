
use std::collections::{BTreeMap, VecDeque};
use std::fmt::{self, Debug, Formatter};
use std::str::FromStr;

use anyhow::{anyhow, bail};
use fastcrypto::encoding::{Encoding, Hex};
use move_binary_format::{
    access::ModuleAccess, binary_views::BinaryIndexedView, file_format::SignatureToken,
    file_format_common::VERSION_MAX,
};
// use move_bytecode_utils::resolve_struct;
use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::IdentStr;
use move_core_types::u256::U256;
use move_core_types::value::MoveFieldLayout;
pub use move_core_types::value::MoveTypeLayout;
use move_core_types::{
    ident_str,
    identifier::Identifier,
    language_storage::{StructTag, TypeTag},
    value::{MoveStruct, MoveStructLayout, MoveValue},
};
// use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Number, Value as JsonValue};

use sui_json::{SuiJsonValue, ResolvedCallArg, primitive_type};

use sui_types::base_types::{
    ObjectID, SuiAddress, TxContext, TxContextKind, RESOLVED_ASCII_STR, RESOLVED_STD_OPTION,
    RESOLVED_UTF8_STR, STD_ASCII_MODULE_NAME, STD_ASCII_STRUCT_NAME, STD_OPTION_MODULE_NAME,
    STD_OPTION_STRUCT_NAME, STD_UTF8_MODULE_NAME, STD_UTF8_STRUCT_NAME,
};
use sui_types::id::{ID, RESOLVED_SUI_ID};
use sui_types::move_package::MovePackage;
use sui_types::MOVE_STDLIB_ADDRESS;
use sui_types::transaction::Argument;

const HEX_PREFIX: &str = "0x";

#[derive(Clone, Debug)]
pub enum ProgrammableTransactionArg {
    Argument(Argument),
    SuiJsonValue(SuiJsonValue) 
}

pub enum ProgrammableTransactionResolvedCallArg {
    Argument(Argument),
    ResolvedCallArg(ResolvedCallArg),
}

pub fn resolve_move_function_args(
    package: &MovePackage,
    module_ident: Identifier,
    function: Identifier,
    type_args: &[TypeTag],
    combined_args_json: Vec<ProgrammableTransactionArg>,
) -> Result<Vec<(ProgrammableTransactionResolvedCallArg, SignatureToken)>, anyhow::Error> {
    // Extract the expected function signature
    let module = package.deserialize_module(&module_ident, VERSION_MAX, true)?;
    let function_str = function.as_ident_str();
    let fdef = module
        .function_defs
        .iter()
        .find(|fdef| {
            module.identifier_at(module.function_handle_at(fdef.function).name) == function_str
        })
        .ok_or_else(|| {
            anyhow!(
                "Could not resolve function {} in module {}",
                function,
                module_ident
            )
        })?;
    let function_signature = module.function_handle_at(fdef.function);
    let parameters = &module.signature_at(function_signature.parameters).0;

    let view = BinaryIndexedView::Module(&module);

    // Lengths have to match, less one, due to TxContext
    let expected_len = match parameters.last() {
        Some(param) if TxContext::kind(&view, param) != TxContextKind::None => parameters.len() - 1,
        _ => parameters.len(),
    };
    if combined_args_json.len() != expected_len {
        bail!(
            "Expected {} args, found {}",
            expected_len,
            combined_args_json.len()
        );
    }
    // Check that the args are valid and convert to the correct format
    let call_args = resolve_call_args(&view, type_args, &combined_args_json, parameters)?;
    let tupled_call_args = call_args
        .into_iter()
        .zip(parameters.iter())
        .map(|(arg, expected_type)| (arg, expected_type.clone()))
        .collect::<Vec<_>>();
    Ok(tupled_call_args)
}

fn resolve_call_args(
    view: &BinaryIndexedView,
    type_args: &[TypeTag],
    json_args: &[ProgrammableTransactionArg],
    parameter_types: &[SignatureToken],
) -> Result<Vec<ProgrammableTransactionResolvedCallArg>, anyhow::Error> {
    json_args
        .iter()
        .zip(parameter_types)
        .enumerate()
        .map(|(idx, (arg, param))| {
            Ok(
                match arg {
                    ProgrammableTransactionArg::Argument(pta) => {
                        ProgrammableTransactionResolvedCallArg::Argument(pta.clone())
                    },
                    ProgrammableTransactionArg::SuiJsonValue(sjv) => {
                        ProgrammableTransactionResolvedCallArg::ResolvedCallArg(resolve_call_arg(view, type_args, idx, sjv, param)?)
                    },
                }
            )
        })
        .collect()
}

fn resolve_call_arg(
    view: &BinaryIndexedView,
    type_args: &[TypeTag],
    idx: usize,
    arg: &SuiJsonValue,
    param: &SignatureToken,
) -> Result<ResolvedCallArg, anyhow::Error> {
    let (is_primitive, layout_opt) = primitive_type(view, type_args, param);
    if is_primitive {
        match layout_opt {
            Some(layout) => {
                return Ok(ResolvedCallArg::Pure(arg.to_bcs_bytes(&layout).map_err(
                    |e| {
                        anyhow!(
                        "Could not serialize argument of type {:?} at {} into {}. Got error: {:?}",
                        param,
                        idx,
                        layout,
                        e
                    )
                    },
                )?));
            }
            None => {
                debug_assert!(
                    false,
                    "Should be unreachable. All primitive type function args \
                     should have a corresponding MoveLayout"
                );
                bail!(
                    "Could not serialize argument of type {:?} at {}",
                    param,
                    idx
                );
            }
        }
    }

    // in terms of non-primitives we only currently support objects and "flat" (depth == 1) vectors
    // of objects (but not, for example, vectors of references)
    match param {
        SignatureToken::Struct(_)
        | SignatureToken::StructInstantiation(_, _)
        | SignatureToken::TypeParameter(_)
        | SignatureToken::Reference(_)
        | SignatureToken::MutableReference(_) => Ok(ResolvedCallArg::Object(resolve_object_arg(
            idx,
            &arg.to_json_value(),
        )?)),
        SignatureToken::Vector(inner) => match &**inner {
            SignatureToken::Struct(_) | SignatureToken::StructInstantiation(_, _) => {
                Ok(ResolvedCallArg::ObjVec(resolve_object_vec_arg(idx, arg)?))
            }
            _ => {
                bail!(
                    "Unexpected non-primitive vector arg {:?} at {} with value {:?}",
                    param,
                    idx,
                    arg
                );
            }
        },
        _ => bail!(
            "Unexpected non-primitive arg {:?} at {} with value {:?}",
            param,
            idx,
            arg
        ),
    }
}

fn resolve_object_arg(idx: usize, arg: &JsonValue) -> Result<ObjectID, anyhow::Error> {
    // Every elem has to be a string convertible to a ObjectID
    match arg {
        JsonValue::String(s) => {
            let s = s.trim().to_lowercase();
            if !s.starts_with(HEX_PREFIX) {
                bail!("ObjectID hex string must start with 0x.",);
            }
            Ok(ObjectID::from_hex_literal(&s)?)
        }
        _ => bail!(
            "Unable to parse arg {:?} as ObjectID at pos {}. Expected {:?}-byte hex string \
                prefixed with 0x.",
            arg,
            idx,
            ObjectID::LENGTH,
        ),
    }
}

fn resolve_object_vec_arg(idx: usize, arg: &SuiJsonValue) -> Result<Vec<ObjectID>, anyhow::Error> {
    // Every elem has to be a string convertible to a ObjectID
    match arg.to_json_value() {
        JsonValue::Array(a) => {
            let mut object_ids = vec![];
            for id in a {
                object_ids.push(resolve_object_arg(idx, &id)?);
            }
            Ok(object_ids)
        }
        JsonValue::String(s) if s.starts_with('[') && s.ends_with(']') => {
            // Due to how escaping of square bracket works, we may be dealing with a JSON string
            // representing a JSON array rather than with the array itself ("[0x42,0x7]" rather than
            // [0x42,0x7]).
            let mut object_ids = vec![];
            for tok in s[1..s.len() - 1].to_string().split(',') {
                let id = JsonValue::String(tok.to_string());
                object_ids.push(resolve_object_arg(idx, &id)?);
            }
            Ok(object_ids)
        }
        _ => bail!(
            "Unable to parse arg {:?} as vector of ObjectIDs at pos {}. \
             Expected a vector of {:?}-byte hex strings prefixed with 0x.\n\
             Consider escaping your curly braces with a backslash (as in \\[0x42,0x7\\]) \
             or enclosing the whole vector in single quotes (as in '[0x42,0x7]')",
            arg.to_json_value(),
            idx,
            ObjectID::LENGTH,
        ),
    }
}