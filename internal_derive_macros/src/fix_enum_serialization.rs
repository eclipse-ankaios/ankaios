// Copyright (c) 2025 Elektrobit Automotive GmbH
//
// This program and the accompanying materials are made available under the
// terms of the Apache License, Version 2.0 which is available at
// https://www.apache.org/licenses/LICENSE-2.0.
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.
//
// SPDX-License-Identifier: Apache-2.0
use proc_macro2::TokenStream;
use quote::{
    //ToTokens,
    format_ident,
    quote,
};
use syn::{Attribute, Data, DeriveInput, Fields, FieldsUnnamed, Type};

use crate::utils;

/// Extracts the original type from an enum variant.
/// Assumes the variant has a single unnamed field (tuple variant).
fn extract_original_type_from_variant(variant: &syn::Variant) -> syn::Result<&syn::Type> {
    match &variant.fields {
        syn::Fields::Unnamed(FieldsUnnamed { unnamed, .. }) if unnamed.len() == 1 => {
            Ok(&unnamed.first().unwrap().ty)
        }
        _ => Err(syn::Error::new_spanned(
            &variant.ident,
            "Enum variant must have exactly one unnamed field",
        )),
    }
}

fn get_name_from_path(type_path: &syn::TypePath) -> syn::Result<String> {
    let type_name = type_path
        .path
        .segments
        .last()
        .ok_or_else(|| syn::Error::new_spanned(type_path, "Type path has no segments"))?
        .ident
        .to_string();
    Ok(type_name)
}

/// Generates a serializer function name from a type path.
fn generate_serializer_name(type_path: &syn::TypePath) -> syn::Result<proc_macro2::Ident> {
    Ok(format_ident!(
        "{}_serializer",
        utils::pascal_to_snake_case(&get_name_from_path(type_path)?)
    ))
}

fn generate_deserializer_name(type_path: &syn::TypePath) -> syn::Result<proc_macro2::Ident> {
    Ok(format_ident!(
        "{}_deserializer",
        utils::pascal_to_snake_case(&get_name_from_path(type_path)?)
    ))
}

fn generate_deserializer(
    fn_name: &proc_macro2::Ident,
    original_type: &syn::Type,
    new_type: &syn::Type,
) -> proc_macro2::TokenStream {
    let converted_value = if utils::is_option_type(original_type) {
        quote! {
            Some(enum_value as i32)
        }
    } else {
        quote! {
            enum_value as i32
        }
    };

    quote! {
        fn #fn_name<'de, D>(
            deserializer: D,
        ) -> Result<#original_type, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            use serde::Deserialize;

            let enum_value = #new_type::deserialize(deserializer)?;
            let new_value = #converted_value;
            Ok(new_value)
        }
    }
}

/// Generates a serializer function for a given type.
/// This function creates a serde serializer that converts the original type to the new type using TryFrom,
/// then serializes it using serde.
fn generate_serializer(
    fn_name: &proc_macro2::Ident,
    original_type: &syn::Type,
    new_type: &syn::Type,
) -> proc_macro2::TokenStream {
    let converted_value = if utils::is_option_type(original_type) {
        quote! {
            value.unwrap_or_default()
        }
    } else {
        quote! {
            *value
        }
    };

    quote! {
        fn #fn_name<S>(
            value: &#original_type,
            serializer: S,
        ) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            use serde::Serialize;
            #new_type::try_from(#converted_value)
                .map_err(serde::ser::Error::custom)?
                .serialize(serializer)
        }
    }
}

/// Generates a serializer function for map fields with enum values.
/// This function creates a serde serializer that converts each value in the map to the new type using TryFrom,
/// then serializes the map using serde.
fn generate_map_serializer(
    fn_name: &proc_macro2::Ident,
    original_type: &syn::Type,
    new_type: &syn::Type,
) -> proc_macro2::TokenStream {
    quote! {
        fn #fn_name<S>(
            value: &#original_type,
            serializer: S,
        ) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            use serde::Serialize;
            let converted_map: std::collections::BTreeMap<_, _> = value.iter()
                .map(|(k, v)| {
                    let converted_v = #new_type::try_from(v.clone())
                        .map_err(serde::ser::Error::custom)?;
                    Ok((k.clone(), converted_v))
                })
                .collect::<Result<_, S::Error>>()?;
            converted_map.serialize(serializer)
        }
    }
}

fn generate_map_deserializer(
    fn_name: &proc_macro2::Ident,
    original_type: &syn::Type,
    new_type: &syn::Type,
) -> proc_macro2::TokenStream {
    quote! {
        fn #fn_name<'de, D>(
            deserializer: D,
        ) -> Result<#original_type, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            use serde::Deserialize;
            let map: std::collections::HashMap<_, #new_type> = Deserialize::deserialize(deserializer)?;
            let converted_map: std::collections::HashMap<_, _> = map.into_iter()
                .map(|(k, v)| {
                    let converted_v = v as i32;
                    Ok((k, converted_v))
                })
                .collect::<Result<_, D::Error>>()?;
            Ok(converted_map)
        }
    }
}

pub fn fix_prost_enum_serialization(mut input: DeriveInput) -> syn::Result<TokenStream> {
    let mut new_functions: Vec<TokenStream> = Vec::new();
    match &mut input.data {
        Data::Struct(data_struct) => {
            match &mut data_struct.fields {
                Fields::Named(fields) => {
                    for field in &mut fields.named {
                        let is_map_field;
                        let new_tp = if let Some(prost_enum_type) =
                            utils::get_prost_enum_type(&field.attrs)
                        {
                            is_map_field = false;
                            prost_enum_type
                        } else if let Some(prost_map_enum_type) =
                            utils::get_prost_map_enum_value_type(&field.attrs)
                        {
                            is_map_field = true;
                            prost_map_enum_type
                        } else {
                            // Not a prost enum field, skip
                            continue;
                        };

                        let ser_fn_name = generate_serializer_name(&new_tp)?;
                        let deser_fn_name = generate_deserializer_name(&new_tp)?;
                        let original_type = &field.ty;
                        let new_type = Type::Path(new_tp);

                        let serializer_fn;
                        let deserializer_fn;

                        if is_map_field {
                            serializer_fn =
                                generate_map_serializer(&ser_fn_name, original_type, &new_type);
                            deserializer_fn =
                                generate_map_deserializer(&deser_fn_name, original_type, &new_type);
                        } else {
                            serializer_fn =
                                generate_serializer(&ser_fn_name, original_type, &new_type);
                            deserializer_fn =
                                generate_deserializer(&deser_fn_name, original_type, &new_type);
                        };
                        new_functions.push(serializer_fn);
                        new_functions.push(deserializer_fn);

                        // Add serde attributes for enum serialization
                        let serializer_fn_name_str = ser_fn_name.to_string();
                        let deserializer_fn_name_str = deser_fn_name.to_string();
                        let serde_attr: Attribute = syn::parse_quote! {
                            #[serde(serialize_with = #serializer_fn_name_str, deserialize_with = #deserializer_fn_name_str)]
                        };
                        field.attrs.push(serde_attr);
                    }
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        input.ident,
                        "fix_enum_serialization only supports named structs",
                    ));
                }
            }
        }
        Data::Enum(data_enum) => {
            for variant in &mut data_enum.variants {
                if let Some(prost_enum_type) = utils::get_prost_enum_type(&variant.attrs) {
                    //create the enum serialization functions
                    let serializer_fn_name = generate_serializer_name(&prost_enum_type)?;
                    let deserializer_fn_name = generate_deserializer_name(&prost_enum_type)?;
                    let original_type = extract_original_type_from_variant(variant)?;
                    let new_type = Type::Path(prost_enum_type);

                    let serializer_fn =
                        generate_serializer(&serializer_fn_name, original_type, &new_type);
                    new_functions.push(serializer_fn);
                    let deserializer_fn =
                        generate_deserializer(&deserializer_fn_name, original_type, &new_type);
                    new_functions.push(deserializer_fn);

                    // Add serde attributes for enum serialization
                    let serializer_fn_name_str = serializer_fn_name.to_string();
                    let deserializer_fn_name_str = deserializer_fn_name.to_string();
                    let serde_attr: Attribute = syn::parse_quote! {
                        #[serde(serialize_with = #serializer_fn_name_str, deserialize_with = #deserializer_fn_name_str)]
                    };
                    variant.attrs.push(serde_attr);
                }
            }
        }
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "fix_enum_serialization does not support unions",
            ));
        }
    };

    // Return the modified item
    let expanded = quote! {
        #input
        #(#new_functions)*
    };

    Ok(expanded)
}
