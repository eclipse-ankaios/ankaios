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
use syn::{Fields, FieldsUnnamed, Ident, Token, Type, Visibility, punctuated::Punctuated};

use crate::utils::{
    DerivedInternal, check_for_forbidden_mandatory_attr, get_doc_attrs, get_internal_field_attrs,
    get_prost_enum_type, has_enum_named_attr, inner_boxed_type_path, is_custom_type_path,
    is_option_type_path, pascal_to_snake_case, to_internal_type,
};

pub fn derive_internal_enum(
    variants: Punctuated<syn::Variant, Token![,]>,
    orig_name: Ident,
    vis: Visibility,
    type_attrs: Vec<TokenStream>,
) -> syn::Result<DerivedInternal> {
    let internal_name = format_ident!("{}Internal", orig_name);
    let mut internal_variants = Vec::new();
    let mut try_from_variants = Vec::new();
    let mut from_variants = Vec::new();

    for variant in variants {
        check_for_forbidden_mandatory_attr(&variant, &variant.attrs)?;

        let variant_ident = &variant.ident;
        let internal_field_attrs = get_internal_field_attrs(&variant.attrs);
        let doc_attrs = get_doc_attrs(&variant.attrs);
        let combined_attrs = internal_field_attrs
            .into_iter()
            .chain(doc_attrs)
            .collect::<Vec<_>>();

        match &variant.fields {
            Fields::Named(_) => {
                return Err(syn::Error::new_spanned(
                    variant_ident,
                    "Variants with named fields are not supported.",
                ));
            }
            Fields::Unnamed(FieldsUnnamed { unnamed, .. }) => {
                let conversion_enum_error_msg =
                    format!("Cannot convert '{orig_name}::{variant_ident}' to internal object.");

                if has_enum_named_attr(&variant.attrs) {
                    if unnamed.len() != 1 {
                        return Err(syn::Error::new_spanned(
                            variant_ident,
                            "Variants with 'internal_enum_named' attribute must have exactly one unnamed field",
                        ));
                    }
                    let field = &unnamed[0];
                    check_for_forbidden_mandatory_attr(&field, &field.attrs)?;

                    let Type::Path(tp) = &field.ty else {
                        return Err(syn::Error::new_spanned(
                            variant_ident,
                            "Only simple type paths are supported in enum fields.",
                        ));
                    };
                    if is_option_type_path(tp) {
                        return Err(syn::Error::new_spanned(
                            variant_ident,
                            "Variants with 'internal_enum_named' attribute cannot have Option types.",
                        ));
                    }

                    let new_ty = if is_custom_type_path(tp) {
                        to_internal_type(tp)
                    } else if let Some(prost_enum_tp) = get_prost_enum_type(&field.attrs) {
                        Type::Path(prost_enum_tp)
                    } else {
                        field.ty.clone()
                    };

                    // the new named field should start with a lowercase letter
                    let variant_name = variant_ident.to_string();
                    let new_field_name = format_ident!("{}", pascal_to_snake_case(&variant_name));

                    internal_variants.push(quote! {
                        #(#combined_attrs )*
                        #variant_ident { #new_field_name: #new_ty }
                    });

                    // Enum::A(String) -> EnumInternal::A { a: String }
                    try_from_variants.push(quote! {
                        #orig_name::#variant_ident( field_0 ) =>
                            #internal_name::#variant_ident{
                                #new_field_name: field_0.try_into().map_err(|_| #conversion_enum_error_msg)?
                            }
                    });

                    // EnumInternal::A { a: String } -> Enum::A(String)
                    from_variants.push(quote! {
                        #internal_name::#variant_ident{ #new_field_name } => #orig_name::#variant_ident( #new_field_name.into() )
                    });
                } else {
                    let mut new_variant = Vec::new();
                    let mut try_fields = Vec::new();
                    let mut from_fields = Vec::new();

                    for (i, field) in unnamed.iter().enumerate() {
                        check_for_forbidden_mandatory_attr(&field, &field.attrs)?;

                        let field_id = format_ident!("field_{i}");
                        let orig_ty = &field.ty;

                        // prepare the try_from and from variants
                        if let Type::Path(tp) = orig_ty {
                            if is_option_type_path(tp) {
                                return Err(syn::Error::new_spanned(
                                    tp,
                                    "Variants with optional attribute are not supported.",
                                ));
                            } else if is_custom_type_path(tp) {
                                let new_ty = to_internal_type(tp);
                                new_variant.push(quote! { #new_ty });

                                try_fields.push(quote! {
                                    #field_id.try_into()?
                                });

                                from_fields.push(quote! {
                                    #field_id.into()
                                });
                            // TODO: think about the order
                            } else if let Some(prost_enum_tp) = get_prost_enum_type(&variant.attrs)
                            {
                                let new_ty = Type::Path(prost_enum_tp);
                                new_variant.push(quote! { #new_ty });

                                try_fields.push(quote! {
                                    #field_id.try_into().map_err(|_| #conversion_enum_error_msg)?
                                });

                                from_fields.push(quote! {
                                    #field_id.into()
                                });
                            // handle custom boxed types
                            } else if let Some(inner) = inner_boxed_type_path(tp)
                                && is_custom_type_path(&inner)
                            {
                                let new_ty = to_internal_type(&inner);
                                new_variant.push(quote! { Box<#new_ty> });
                                try_fields.push(quote! {
                                    Box::new((*#field_id).try_into()?)
                                });
                                from_fields.push(quote! {
                                    Box::new((*#field_id).into())
                                });
                            } else {
                                new_variant.push(quote! { #orig_ty });

                                try_fields.push(quote! {
                                    #field_id
                                });

                                from_fields.push(quote! {
                                    #field_id
                                });
                            }
                        } // TODO: else panic?
                    }

                    internal_variants.push(quote! {
                        #(#combined_attrs )*
                        #variant_ident ( #(#new_variant),* )
                    });

                    // create a vector field_<i> for each unnamed field
                    let bindings = (0..unnamed.len())
                        .map(|i| format_ident!("field_{i}"))
                        .collect::<Vec<_>>();

                    try_from_variants.push(quote! {
                                #orig_name::#variant_ident( #(#bindings),* ) => #internal_name::#variant_ident( #(#try_fields),* )
                            });

                    from_variants.push(quote! {
                                #internal_name::#variant_ident( #(#bindings),* ) => #orig_name::#variant_ident( #(#from_fields),* )
                            });
                };
            }

            Fields::Unit => {
                internal_variants.push(quote! {
                    #(#combined_attrs )*
                    #variant_ident
                });

                try_from_variants.push(quote! {
                    #orig_name::#variant_ident => #internal_name::#variant_ident
                });

                from_variants.push(quote! {
                    #internal_name::#variant_ident => #orig_name::#variant_ident
                });
            }
        };
    }

    let internal_enum = quote! {
        #(#type_attrs )*
        #vis enum #internal_name {
            #(#internal_variants,)*
        }
    };

    let try_from_impl = quote! {
        impl std::convert::TryFrom<#orig_name> for #internal_name {
            type Error = String;

            fn try_from(orig: #orig_name) -> Result<Self, Self::Error> {
                Ok(match orig {
                    #(#try_from_variants),*
                })
            }
        }
    };

    let from_impl = quote! {
        impl From<#internal_name> for #orig_name {
            fn from(original: #internal_name) -> Self {
                match original {
                    #(#from_variants),*
                }
            }
        }
    };

    Ok(DerivedInternal {
        obj: internal_enum,
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

    use proc_macro2::TokenStream;
    use quote::{format_ident, quote};
    use syn::{Visibility, parse_quote};

    use super::derive_internal_enum;

    #[test]
    fn test_derive_internal_enum_simple() {
        let variants: syn::punctuated::Punctuated<syn::Variant, syn::Token![,]> = parse_quote! {
            VariantA,
            VariantB(u32),
            VariantC(String, bool),
        };

        let orig_name = format_ident!("TestEnum");
        let vis: Visibility = parse_quote! { pub };
        let type_attrs: Vec<TokenStream> = vec![quote! { #[derive(Debug)] }];

        let derived = derive_internal_enum(
            variants,
            orig_name.clone(),
            vis.clone(),
            type_attrs.clone(),
        )
        .unwrap();

        let expected_internal_enum = quote! {
            #(#type_attrs )*
            #vis enum TestEnumInternal {
                VariantA,
                VariantB(u32),
                VariantC(String, bool),
            }
        };

        let expected_try_from_impl = quote! {
            impl std::convert::TryFrom<TestEnum> for TestEnumInternal {
                type Error = String;

                fn try_from(orig: TestEnum) -> Result<Self, Self::Error> {
                    Ok(match orig {
                        TestEnum::VariantA => TestEnumInternal::VariantA,
                        TestEnum::VariantB( field_0 ) => TestEnumInternal::VariantB( field_0 ),
                        TestEnum::VariantC( field_0, field_1 ) => TestEnumInternal::VariantC( field_0, field_1 )
                    })
                }
            }
        };

        let expected_from_impl = quote! {
            impl From<TestEnumInternal> for TestEnum {
                fn from(original: TestEnumInternal) -> Self {
                    match original {
                        TestEnumInternal::VariantA => TestEnum::VariantA,
                        TestEnumInternal::VariantB( field_0 ) => TestEnum::VariantB( field_0 ),
                        TestEnumInternal::VariantC( field_0, field_1 ) => TestEnum::VariantC( field_0, field_1 )
                    }
                }
            }
        };

        assert_eq!(derived.obj.to_string(), expected_internal_enum.to_string());
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
    fn test_derive_internal_enum_with_named_variant() {
        let variants: syn::punctuated::Punctuated<syn::Variant, syn::Token![,]> = parse_quote! {
            #[internal_enum_named]
            VariantA(String),
            VariantB(u32),
            #[internal_enum_named]
            VariantC(MyType)
        };
        let orig_name = format_ident!("TestEnum");
        let vis: Visibility = parse_quote! { pub };
        let type_attrs: Vec<TokenStream> = vec![quote! { #[derive(Debug)] }];

        let derived = derive_internal_enum(
            variants,
            orig_name.clone(),
            vis.clone(),
            type_attrs.clone(),
        )
        .unwrap();

        let expected_internal_enum = quote! {
            #(#type_attrs )*
            #vis enum TestEnumInternal {
                VariantA { variant_a: String },
                VariantB(u32),
                VariantC { variant_c: MyTypeInternal },
            }
        };

        let expected_try_from_impl = quote! {
            impl std::convert::TryFrom<TestEnum> for TestEnumInternal {
                type Error = String;

                fn try_from(orig: TestEnum) -> Result<Self, Self::Error> {
                    Ok(match orig {
                        TestEnum::VariantA( field_0 ) =>
                            TestEnumInternal::VariantA{
                                variant_a: field_0.try_into().map_err(|_| "Cannot convert 'TestEnum::VariantA' to internal object.")?
                            },
                        TestEnum::VariantB( field_0 ) => TestEnumInternal::VariantB( field_0 ),
                        TestEnum::VariantC( field_0 ) =>
                            TestEnumInternal::VariantC{
                                variant_c: field_0.try_into().map_err(|_| "Cannot convert 'TestEnum::VariantC' to internal object.")?
                            }
                    })
                }
            }
        };

        let expected_from_impl = quote! {
            impl From<TestEnumInternal> for TestEnum {
                fn from(original: TestEnumInternal) -> Self {
                    match original {
                        TestEnumInternal::VariantA{ variant_a } => TestEnum::VariantA( variant_a.into() ),
                        TestEnumInternal::VariantB( field_0 ) => TestEnum::VariantB( field_0 ),
                        TestEnumInternal::VariantC{ variant_c } => TestEnum::VariantC( variant_c.into() )
                    }
                }
            }
        };

        assert_eq!(derived.obj.to_string(), expected_internal_enum.to_string());
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
    fn test_derive_internal_enum_error_on_named_fields() {
        let variants: syn::punctuated::Punctuated<syn::Variant, syn::Token![,]> = parse_quote! {
            VariantA { field_a: u32 },
            VariantB(u32),
        };
        let orig_name = format_ident!("TestEnum");
        let vis: Visibility = parse_quote! { pub };
        let type_attrs: Vec<TokenStream> = vec![quote! { #[derive(Debug)] }];
        let result = derive_internal_enum(variants, orig_name, vis, type_attrs);

        assert!(result.is_err());
    }

    #[test]
    fn test_derive_internal_enum_error_on_option_in_named_variant() {
        let variants: syn::punctuated::Punctuated<syn::Variant, syn::Token![,]> = parse_quote! {
            #[internal_enum_named]
            VariantA(Option<String>),
            VariantB(u32),
        };
        let orig_name = format_ident!("TestEnum");
        let vis: Visibility = parse_quote! { pub };
        let type_attrs: Vec<TokenStream> = vec![quote! { #[derive(Debug)] }];
        let result = derive_internal_enum(variants, orig_name, vis, type_attrs);
        assert!(result.is_err());
    }

    #[test]
    fn test_derive_internal_enum_error_on_option_in_variant() {
        let variants: syn::punctuated::Punctuated<syn::Variant, syn::Token![,]> = parse_quote! {
            VariantA(Option<String>),
            VariantB(u32),
        };
        let orig_name = format_ident!("TestEnum");
        let vis: Visibility = parse_quote! { pub };
        let type_attrs: Vec<TokenStream> = vec![quote! { #[derive(Debug)] }];
        let result = derive_internal_enum(variants, orig_name, vis, type_attrs);
        assert!(result.is_err());
    }

    #[test]
    fn test_derive_internal_enum_prost_enum_type() {
        let variants: syn::punctuated::Punctuated<syn::Variant, syn::Token![,]> = parse_quote! {
            #[prost(enumeration = "MyEnum", tag = "2")]
            VariantA(u32),
            VariantB(u32),
        };
        let orig_name = format_ident!("TestEnum");
        let vis: Visibility = parse_quote! { pub };
        let type_attrs: Vec<TokenStream> = vec![quote! { #[derive(Debug)] }];

        let derived = derive_internal_enum(
            variants,
            orig_name.clone(),
            vis.clone(),
            type_attrs.clone(),
        )
        .unwrap();

        let expected_internal_enum = quote! {
            #(#type_attrs )*
            #vis enum TestEnumInternal {
                VariantA(MyEnum),
                VariantB(u32),
            }
        };

        let expected_try_from_impl = quote! {
            impl std::convert::TryFrom<TestEnum> for TestEnumInternal {
                type Error = String;

                fn try_from(orig: TestEnum) -> Result<Self, Self::Error> {
                    Ok(match orig {
                        TestEnum::VariantA( field_0 ) => TestEnumInternal::VariantA( field_0.try_into().map_err(|_| "Cannot convert 'TestEnum::VariantA' to internal object.")? ),
                        TestEnum::VariantB( field_0 ) => TestEnumInternal::VariantB( field_0 )
                    })
                }
            }
        };

        let expected_from_impl = quote! {
            impl From<TestEnumInternal> for TestEnum {
                fn from(original: TestEnumInternal) -> Self {
                    match original {
                        TestEnumInternal::VariantA( field_0 ) => TestEnum::VariantA( field_0.into() ),
                        TestEnumInternal::VariantB( field_0 ) => TestEnum::VariantB( field_0 )
                    }
                }
            }
        };

        assert_eq!(derived.obj.to_string(), expected_internal_enum.to_string());
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
    fn test_derive_internal_enum_with_docs() {
        let variants: syn::punctuated::Punctuated<syn::Variant, syn::Token![,]> = parse_quote! {
            /// This is variant A
            VariantA,
            /// This is variant B
            VariantB(u32),
        };
        let orig_name = format_ident!("TestEnum");
        let vis: Visibility = parse_quote! { pub };
        let type_attrs: Vec<TokenStream> = vec![quote! { #[derive(Debug)] }];
        let derived = derive_internal_enum(
            variants,
            orig_name.clone(),
            vis.clone(),
            type_attrs.clone(),
        )
        .unwrap();

        let expected_internal_enum = quote! {
            #(#type_attrs )*
            #vis enum TestEnumInternal {
                /// This is variant A
                VariantA,
                /// This is variant B
                VariantB(u32),
            }
        };
        assert_eq!(derived.obj.to_string(), expected_internal_enum.to_string());
    }
}
