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
use quote::{format_ident, quote};
use syn::{FieldsNamed, Ident, Type, Visibility, parse_quote};

use crate::derive_internal::utils::{
    DerivedInternal, get_internal_field_attrs, get_prost_enum_type, is_custom_type_path,
    is_option_type_path, to_internal_type, has_mandatory_attr, get_prost_map_enum_value_type,
    inner_vec_type_path, inner_hashmap_type_path, wrap_in_option, extract_inner
};

pub fn derive_internal_struct(
    fields_named: FieldsNamed,
    orig_name: Ident,
    vis: Visibility,
    type_attrs: Vec<TokenStream>,
    skip_try_from: bool,
) -> syn::Result<DerivedInternal> {
    let internal_name = format_ident!("{}Internal", orig_name);

    let mut internal_fields = Vec::new();
    let mut try_from_inits = Vec::new();
    let mut from_inits = Vec::new();

    for field in fields_named.named {
        let field_vis = field.vis.clone();
        let field_name = field.ident.unwrap();

        let Type::Path(tp) = &field.ty else {
            return Err(syn::Error::new_spanned(
                field_name,
                "Only simple type paths are supported in struct fields.",
            ));
        };

        let missing_field_msg = format!("Missing field '{field_name}'");
        let conversion_error_msg =
            format!("Cannot convert field '{field_name}' to internal object.");

        let mandatory = has_mandatory_attr(&field.attrs);

        let prost_enum_tp = get_prost_enum_type(&field.attrs);

        let prost_map_enum_value_tp = get_prost_map_enum_value_type(&field.attrs);

        let new_field_type: Type;
        let try_from_init_entry;
        let from_init_entry;

        if is_option_type_path(tp) {
            // Option<inner>
            let inner = extract_inner(tp);
            if mandatory {
                if prost_enum_tp.is_some() || is_custom_type_path(&inner) {
                    new_field_type = if let Some(prost_enum_type) = prost_enum_tp {
                        Type::Path(prost_enum_type)
                    } else {
                        to_internal_type(&inner)
                    };

                    try_from_init_entry = quote! {
                        #field_name: orig.#field_name
                            .ok_or(#missing_field_msg)?
                            .try_into()
                            .map_err(|_| #conversion_error_msg)?
                    };
                    from_init_entry = quote! {
                        #field_name: Some(orig.#field_name.into())
                    };
                } else {
                    new_field_type = Type::Path(inner);

                    try_from_init_entry = quote! {
                        #field_name: orig.#field_name
                            .ok_or(#missing_field_msg)?
                    };
                    from_init_entry = quote! {
                        #field_name: Some(orig.#field_name)
                    };
                }
            } else if prost_enum_tp.is_some() || is_custom_type_path(&inner) {
                new_field_type = wrap_in_option(if let Some(prost_enum_type) = prost_enum_tp {
                    Type::Path(prost_enum_type)
                } else {
                    to_internal_type(&inner)
                });

                try_from_init_entry = quote! {
                    #field_name: orig.#field_name.map(|v| v.try_into()).transpose()?
                };
                from_init_entry = quote! {
                    #field_name: orig.#field_name.map(|v| v.into())
                };
            } else {
                new_field_type = field.ty.clone();

                try_from_init_entry = quote! {
                    #field_name: orig.#field_name
                };
                from_init_entry = quote! {
                    #field_name: orig.#field_name
                };
            }
        } else {
            // not an option
            if prost_enum_tp.is_some() || is_custom_type_path(tp) {
                new_field_type = if let Some(prost_enum_type) = prost_enum_tp {
                    Type::Path(prost_enum_type)
                } else {
                    to_internal_type(tp)
                };

                try_from_init_entry = quote! {
                    #field_name: orig.#field_name.try_into().map_err(|_| #conversion_error_msg)?
                };
                from_init_entry = quote! {
                    #field_name: orig.#field_name.into()
                };
            } else if let Some(inner) = inner_vec_type_path(tp)
                && is_custom_type_path(&inner)
            {
                let new_inner = to_internal_type(&inner);
                new_field_type = Type::Path(parse_quote! { Vec<#new_inner> });

                try_from_init_entry = quote! {
                    #field_name: orig.#field_name.into_iter().map(|v| v.try_into()).collect::<Result<_, _>>()?
                };
                from_init_entry = quote! {
                    #field_name: orig.#field_name.into_iter().map(|v| v.into()).collect()
                };
            } else if let Some((key_tp, val_tp)) = inner_hashmap_type_path(tp)
                && (is_custom_type_path(&val_tp) || prost_map_enum_value_tp.is_some())
            {
                let new_val_tp = if let Some(prost_map_enum_value_tp) = prost_map_enum_value_tp {
                    Type::Path(prost_map_enum_value_tp)
                } else {
                    to_internal_type(&val_tp)
                };
                new_field_type =
                    Type::Path(parse_quote! { ::std::collections::HashMap<#key_tp, #new_val_tp> });

                try_from_init_entry = quote! {
                    #field_name: orig.#field_name.into_iter()
                        .map(|(k, v)| Ok((k.clone(), v.try_into().map_err(|_| #conversion_error_msg)?)))
                        .collect::<Result<_, String>>()?
                };
                from_init_entry = quote! {
                    #field_name: orig.#field_name.into_iter().map(|(k, v)| (k.clone(), v.into())).collect()
                };
            } else {
                new_field_type = field.ty.clone();

                try_from_init_entry = quote! {
                    #field_name: orig.#field_name
                };
                from_init_entry = quote! {
                    #field_name: orig.#field_name
                };
            }
        }

        let internal_field_attrs = get_internal_field_attrs(&field.attrs);
        internal_fields.push(quote! {
            #(#internal_field_attrs )*
            #field_vis #field_name: #new_field_type
        });

        try_from_inits.push(try_from_init_entry);
        from_inits.push(from_init_entry);
    }

    let internal_struct = quote! {
        #(#type_attrs )*
        #vis struct #internal_name {
            #(#internal_fields, )*
        }
    };

    let try_from_impl = if skip_try_from {
        TokenStream::new()
    } else {
        quote! {
            impl std::convert::TryFrom<#orig_name> for #internal_name {
                type Error = String;

                fn try_from(orig: #orig_name) -> Result<Self, Self::Error> {
                    Ok(#internal_name {
                        #(#try_from_inits, )*
                    })
                }
            }
        }
    };

    let from_impl = quote! {
        impl From<#internal_name> for #orig_name {
            fn from(orig: #internal_name) -> Self {
                #orig_name {
                    #(#from_inits, )*
                }
            }
        }
    };

    Ok(DerivedInternal {
        obj: internal_struct,
        try_from_impl,
        from_impl,
    })
}
