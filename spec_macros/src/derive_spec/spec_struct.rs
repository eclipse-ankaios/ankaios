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

use crate::utils::{
    DerivedSpec, extract_inner, get_doc_attrs, get_option_handling, get_prost_enum_type,
    get_prost_map_enum_value_type, get_spec_field_attrs, inner_hashmap_type_path,
    inner_vec_type_path, is_custom_type_path, is_option_type_path, to_spec_ident, to_spec_type,
    wrap_in_option,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{FieldsNamed, Ident, Type, Visibility, parse_quote};

pub fn derive_spec_struct(
    fields_named: FieldsNamed,
    orig_name: Ident,
    vis: Visibility,
    type_attrs: Vec<TokenStream>,
) -> syn::Result<DerivedSpec> {
    let spec_name = to_spec_ident(&orig_name);
    let mut spec_fields = Vec::new();
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

        let conversion_error_msg =
            format!("Cannot convert field '{field_name}' to spec object: ") + "'{err}'.";

        let option_handling = get_option_handling(&field.attrs, &field_name);

        let prost_enum_tp = get_prost_enum_type(&field.attrs);

        let prost_map_enum_value_tp = get_prost_map_enum_value_type(&field.attrs);

        let new_field_type: Type;
        let try_from_init_entry;
        let from_init_entry;

        if is_option_type_path(tp) {
            // Option<inner>
            let inner = extract_inner(tp);
            if let Some(option_handling) = option_handling {
                if prost_enum_tp.is_some() || is_custom_type_path(&inner) {
                    new_field_type = if let Some(prost_enum_type) = prost_enum_tp {
                        Type::Path(prost_enum_type)
                    } else {
                        to_spec_type(&inner)
                    };

                    try_from_init_entry = quote! {
                        #field_name: orig.#field_name
                            #option_handling
                            .try_into()
                            .map_err(|err| format!(#conversion_error_msg))?
                    };
                    from_init_entry = quote! {
                        #field_name: Some(orig.#field_name.into())
                    };
                } else {
                    new_field_type = Type::Path(inner);

                    try_from_init_entry = quote! {
                        #field_name: orig.#field_name
                            #option_handling
                    };
                    from_init_entry = quote! {
                        #field_name: Some(orig.#field_name)
                    };
                }
            } else if prost_enum_tp.is_some() || is_custom_type_path(&inner) {
                new_field_type = wrap_in_option(if let Some(prost_enum_type) = prost_enum_tp {
                    Type::Path(prost_enum_type)
                } else {
                    to_spec_type(&inner)
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
                    to_spec_type(tp)
                };

                try_from_init_entry = quote! {
                    #field_name: orig.#field_name.try_into().map_err(|err| format!(#conversion_error_msg))?
                };
                from_init_entry = quote! {
                    #field_name: orig.#field_name.into()
                };
            } else if let Some(inner) = inner_vec_type_path(tp)
                && is_custom_type_path(&inner)
            {
                let new_inner = to_spec_type(&inner);
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
                    to_spec_type(&val_tp)
                };
                new_field_type =
                    Type::Path(parse_quote! { ::std::collections::HashMap<#key_tp, #new_val_tp> });

                try_from_init_entry = quote! {
                    #field_name: orig.#field_name.into_iter()
                        .map(|(k, v)| Ok((k.clone(), v.try_into().map_err(|err| format!(#conversion_error_msg))?)))
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

        let spec_field_attrs = get_spec_field_attrs(&field.attrs);
        let doc_attrs = get_doc_attrs(&field.attrs);
        let combined_attrs = spec_field_attrs
            .into_iter()
            .chain(doc_attrs)
            .collect::<Vec<_>>();

        spec_fields.push(quote! {
            #(#combined_attrs )*
            #field_vis #field_name: #new_field_type
        });

        try_from_inits.push(try_from_init_entry);
        from_inits.push(from_init_entry);
    }

    let spec_struct = quote! {
        #(#type_attrs )*
        #vis struct #spec_name {
            #(#spec_fields, )*
        }
    };

    let try_from_impl = quote! {
        impl std::convert::TryFrom<#orig_name> for #spec_name {
            type Error = String;

            fn try_from(orig: #orig_name) -> Result<Self, Self::Error> {
                Ok(#spec_name {
                    #(#try_from_inits, )*
                })
            }
        }
    };

    let from_impl = quote! {
        impl From<#spec_name> for #orig_name {
            fn from(orig: #spec_name) -> Self {
                #orig_name {
                    #(#from_inits, )*
                }
            }
        }
    };

    Ok(DerivedSpec {
        obj: spec_struct,
        try_from_impl,
        from_impl,
    })
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use crate::derive_spec::spec_struct::derive_spec_struct;

    use proc_macro2::TokenStream;
    use quote::quote;
    use syn::{Ident, Visibility, parse_quote};

    #[test]
    fn test_derive_spec_struct_with_mandatory() {
        use syn::{FieldsNamed, parse_quote};

        let fields_named: FieldsNamed = parse_quote! {
            {
                #[spec_mandatory]
                pub field1: Option<CustomType>,
                pub field2: Vec<CustomType>,
                pub field3: std::collections::HashMap<String, CustomType>,
                pub field4: i32,
            }
        };

        let orig_name: Ident = parse_quote! { MyStruct };
        let vis: Visibility = parse_quote! { pub };
        let type_attrs: Vec<TokenStream> = vec![quote! { #[derive(Debug)] }];

        let derived = derive_spec_struct(fields_named, orig_name, vis, type_attrs).unwrap();

        let expected_obj = quote! {
            #[derive(Debug)]
            pub struct MyStructSpec {
                pub field1: CustomTypeSpec,
                pub field2: Vec<CustomTypeSpec>,
                pub field3: ::std::collections::HashMap<String, CustomTypeSpec>,
                pub field4: i32,
            }
        };

        assert_eq!(derived.obj.to_string(), expected_obj.to_string());
        assert_eq!(
        derived.try_from_impl.to_string(),
        quote! {
            impl std::convert::TryFrom<MyStruct> for MyStructSpec {
                type Error = String;

                fn try_from(orig: MyStruct) -> Result<Self, Self::Error> {
                    Ok(MyStructSpec {
                        field1: orig.field1.ok_or("Missing field 'field1'")?.try_into().map_err(| err | format ! ("Cannot convert field 'field1' to spec object: '{err}'."))?,
                        field2: orig.field2.into_iter().map(|v| v.try_into()).collect::<Result<_, _>>()?,
                        field3: orig.field3.into_iter()
                            .map(|(k, v)| Ok((k.clone(), v.try_into().map_err(| err | format ! ("Cannot convert field 'field3' to spec object: '{err}'."))?)))
                            .collect::<Result<_, String>>()?,
                        field4: orig.field4,
                    })
                }
            }
        }
        .to_string()
    );
        assert_eq!(
        derived.from_impl.to_string(),
        quote! {
            impl From<MyStructSpec> for MyStruct {
                fn from(orig: MyStructSpec) -> Self {
                    MyStruct {
                        field1: Some(orig.field1.into()),
                        field2: orig.field2.into_iter().map(|v| v.into()).collect(),
                        field3: orig.field3.into_iter().map(|(k, v)| (k.clone(), v.into())).collect(),
                        field4: orig.field4,
                    }
                }
            }
        }
        .to_string()
    );
    }

    #[test]
    fn test_derive_spec_struct_with_default() {
        use syn::{FieldsNamed, parse_quote};
        let fields_named: FieldsNamed = parse_quote! {
            {
                #[spec_default]
                pub field1: Option<i32>,
                #[spec_default(42)]
                pub field2: Option<i32>,
                #[spec_default(vec![42, 42, 42])]
                pub field3: Option<Vec<i32>>,
            }
        };
        let orig_name: Ident = parse_quote! { MyStruct };
        let vis: Visibility = parse_quote! { pub };
        let type_attrs: Vec<TokenStream> = vec![];

        let derived = derive_spec_struct(fields_named, orig_name, vis, type_attrs).unwrap();
        let expected_obj = quote! {
            pub struct MyStructSpec {
                pub field1: i32,
                pub field2: i32,
                pub field3: Vec<i32>,
            }
        };

        assert_eq!(derived.obj.to_string(), expected_obj.to_string());
        assert_eq!(
            derived.try_from_impl.to_string(),
            quote! {
                impl std::convert::TryFrom<MyStruct> for MyStructSpec {
                    type Error = String;
                    fn try_from(orig: MyStruct) -> Result<Self, Self::Error> {
                        Ok(MyStructSpec {
                            field1: orig.field1.unwrap_or_default(),
                            field2: orig.field2.unwrap_or(42),
                            field3: orig.field3.unwrap_or(vec![42, 42, 42]),
                        })
                    }
                }
            }
            .to_string()
        );
        assert_eq!(
            derived.from_impl.to_string(),
            quote! {
                impl From<MyStructSpec> for MyStruct {
                    fn from(orig: MyStructSpec) -> Self {
                        MyStruct {
                            field1: Some(orig.field1),
                            field2: Some(orig.field2),
                            field3: Some(orig.field3),
                        }
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn test_derive_spec_struct_without_mandatory() {
        use syn::{FieldsNamed, parse_quote};
        let fields_named: FieldsNamed = parse_quote! {
            {
                pub field1: Option<CustomType>,
                pub field2: Vec<CustomType>,
                pub field3: std::collections::HashMap<String, CustomType>,
                pub field4: i32,
            }
        };
        let orig_name: Ident = parse_quote! { MyStruct };
        let vis: Visibility = parse_quote! { pub };
        let type_attrs: Vec<TokenStream> = vec![quote! { #[derive(Debug)] }];

        let derived = derive_spec_struct(fields_named, orig_name, vis, type_attrs).unwrap();

        let expected_obj = quote! {
            #[derive(Debug)]
            pub struct MyStructSpec {
                pub field1: Option<CustomTypeSpec>,
                pub field2: Vec<CustomTypeSpec>,
                pub field3: ::std::collections::HashMap<String, CustomTypeSpec>,
                pub field4: i32,
            }
        };

        assert_eq!(derived.obj.to_string(), expected_obj.to_string());
        assert_eq!(
            derived.try_from_impl.to_string(),
            quote! {
                impl std::convert::TryFrom<MyStruct> for MyStructSpec {
                    type Error = String;

                    fn try_from(orig: MyStruct) -> Result<Self, Self::Error> {
                        Ok(MyStructSpec {
                            field1: orig.field1.map(|v| v.try_into()).transpose()?,
                            field2: orig.field2.into_iter().map(|v| v.try_into()).collect::<Result<_, _>>()?,
                            field3: orig.field3.into_iter()
                                .map(|(k, v)| Ok((k.clone(), v.try_into().map_err(| err | format ! ("Cannot convert field 'field3' to spec object: '{err}'."))?)))
                                .collect::<Result<_, String>>()?,
                            field4: orig.field4,
                        })
                    }
                }
            }
            .to_string()
        );
        assert_eq!(
            derived.from_impl.to_string(),
            quote! {
                impl From<MyStructSpec> for MyStruct {
                    fn from(orig: MyStructSpec) -> Self {
                        MyStruct {
                            field1: orig.field1.map(|v| v.into()),
                            field2: orig.field2.into_iter().map(|v| v.into()).collect(),
                            field3: orig.field3.into_iter().map(|(k, v)| (k.clone(), v.into())).collect(),
                            field4: orig.field4,
                        }
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn test_derive_spec_struct_spec_field_attributes() {
        use syn::{FieldsNamed, parse_quote};
        let fields_named: FieldsNamed = parse_quote! {
            {
                #[spec_field_attr(#[serde(rename = "custom_field1")])]
                pub field1: i32,
            }
        };
        let orig_name: Ident = parse_quote! { MyStruct };
        let vis: Visibility = parse_quote! { pub };
        let type_attrs: Vec<TokenStream> = vec![];

        let derived = derive_spec_struct(fields_named, orig_name, vis, type_attrs).unwrap();

        let expected_obj = quote! {
            pub struct MyStructSpec {
                #[serde(rename = "custom_field1")]
                pub field1: i32,
            }
        };

        assert_eq!(derived.obj.to_string(), expected_obj.to_string());
    }

    #[test]
    fn test_derive_spec_enum_prost_map_with_enum_type() {
        use syn::{FieldsNamed, parse_quote};
        let fields_named: FieldsNamed = parse_quote! {
            {
                #[prost(map = "string, enumeration(CustomEnum)", tag = "1")]
                pub field1: std::collections::HashMap<String, CustomEnum>,
            }
        };
        let orig_name: Ident = parse_quote! { MyStruct };
        let vis: Visibility = parse_quote! { pub };
        let type_attrs: Vec<TokenStream> = vec![];

        let derived = derive_spec_struct(fields_named, orig_name, vis, type_attrs).unwrap();

        let expected_obj = quote! {
            pub struct MyStructSpec {
                pub field1: ::std::collections::HashMap<String, CustomEnum>,
            }
        };

        let expected_try_from_impl = quote! {
            impl std::convert::TryFrom<MyStruct> for MyStructSpec {
                type Error = String;

                fn try_from(orig: MyStruct) -> Result<Self, Self::Error> {
                    Ok(MyStructSpec {
                        field1: orig.field1.into_iter()
                            .map(|(k , v)| Ok ((k.clone (), v.try_into().map_err(| err | format ! ("Cannot convert field 'field1' to spec object: '{err}'."))?)))
                            .collect::<Result<_, String >>() ?,
                        })
                }


            }
        };

        let expected_from_impl = quote! {
            impl From<MyStructSpec> for MyStruct {
                fn from(orig: MyStructSpec) -> Self {
                    MyStruct {
                        field1: orig.field1.into_iter().map(|(k, v)| (k.clone(), v.into())).collect(),
                    }
                }
            }
        };

        assert_eq!(derived.obj.to_string(), expected_obj.to_string());
        assert_eq!(
            derived.try_from_impl.to_string(),
            expected_try_from_impl.to_string()
        );
        assert_eq!(
            derived.from_impl.to_string(),
            expected_from_impl.to_string()
        );
    }

    #[test]
    fn test_derive_spec_struct_preserves_doc_comments() {
        use syn::{FieldsNamed, parse_quote};
        let fields_named: FieldsNamed = parse_quote! {
            {
                /// This is a test field
                pub field1: i32,
            }
        };
        let orig_name: Ident = parse_quote! { MyStruct };
        let vis: Visibility = parse_quote! { pub };
        let type_attrs: Vec<TokenStream> = vec![];
        let derived = derive_spec_struct(fields_named, orig_name, vis, type_attrs).unwrap();
        let expected_obj = quote! {
            pub struct MyStructSpec {
                /// This is a test field
                pub field1: i32,
            }
        };

        assert_eq!(derived.obj.to_string(), expected_obj.to_string());
    }
}
