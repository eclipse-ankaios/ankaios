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

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Attribute, Data, DataEnum, DataStruct, DeriveInput, Fields, FieldsUnnamed, GenericArgument,
    Ident, Meta, Path, PathArguments, Token, Type, TypePath,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
};

pub fn derive_internal(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let orig_name = input.ident;
    let vis = input.vis.clone();
    let internal_name = format_ident!("{}Internal", orig_name);

    let mut internal_derives = Vec::new();
    for attr in &input.attrs {
        if let Meta::List(meta_list) = &attr.meta {
            if meta_list.path.is_ident("internal_derive") {
                let parsed = syn::parse2::<DeriveList>(meta_list.tokens.clone()).unwrap();
                internal_derives.extend(parsed.0.into_iter());
            }
        }
    }
    let internal_quoted_derives = if !internal_derives.is_empty() {
        Some(quote! { #[derive(#(#internal_derives),*)] })
    } else {
        None
    };

    let mut internal_type_attrs = Vec::new();
    for attr in &input.attrs {
        if let Meta::List(meta_list) = &attr.meta {
            if meta_list.path.is_ident("internal_type_attr") {
                internal_type_attrs.push(meta_list.tokens.clone());
            }
        }
    }

    let internal_quoted_type_attrs = if !internal_type_attrs.is_empty() {
        Some(quote! { #(#internal_type_attrs )* })
    } else {
        None
    };

    println!(
        "internal_quoted_type_attrs: {}",
        internal_quoted_type_attrs.clone().unwrap_or_default()
    );

    match input.data {
        Data::Struct(DataStruct {
            fields: Fields::Named(fields_named),
            ..
        }) => {
            let mut internal_fields = Vec::new();
            let mut try_from_inits = Vec::new();
            let mut from_inits = Vec::new();

            for field in fields_named.named {
                let field_name = field.ident.unwrap();
                let mandatory = has_mandatory_attr(&field.attrs);
                let new_ty = transform_type(&field.ty, mandatory);
                let missing_field_msg = format!("Missing field '{field_name}'");

                // TODO: check what the fell the AI did here...
                let internal_field_attrs = get_internal_field_attrs(&field.attrs);

                // add field to Internal struct
                internal_fields.push(quote! {
                    #(#internal_field_attrs )*
                    pub #field_name: #new_ty
                });

                // TODO: this looks way to complicated, we need so simplify it according to the use-case at hand
                // The current general solution is too complex and hard to maintain for our purposes
                // conversion logic
                if let Type::Path(tp) = &field.ty {
                    if is_option(tp) {
                        let inner = extract_inner(tp);
                        if mandatory {
                            if is_custom_type(&inner) {
                                try_from_inits.push(quote! {
                                    #field_name: orig.#field_name
                                        .ok_or(#missing_field_msg)?
                                        .try_into()?
                                });
                                from_inits.push(quote! {
                                    #field_name: Some(orig.#field_name.into())
                                });
                            } else {
                                try_from_inits.push(quote! {
                                    #field_name: orig.#field_name
                                        .ok_or(#missing_field_msg)?
                                });
                                from_inits.push(quote! {
                                    #field_name: Some(orig.#field_name)
                                });
                            }
                        } else if is_custom_type(&inner) {
                            try_from_inits.push(quote! {
                                #field_name: match orig.#field_name {
                                    Some(v) => Some(v.try_into()?),
                                    None => None,
                                }
                            });
                            from_inits.push(quote! {
                                #field_name: orig.#field_name.map(|v| v.into())
                            });
                        } else {
                            try_from_inits.push(quote! {
                                #field_name: orig.#field_name
                            });
                            from_inits.push(quote! {
                                #field_name: orig.#field_name
                            });
                        }
                    } else {
                        // plain type
                        if is_custom_type_path(tp) {
                            if mandatory {
                                try_from_inits.push(quote! {
                                    #field_name: orig.#field_name.try_into()?
                                });
                                from_inits.push(quote! {
                                    #field_name: orig.#field_name.into()
                                });
                            } else {
                                try_from_inits.push(quote! {
                                    #field_name: Some(orig.#field_name.try_into()?)
                                });
                                from_inits.push(quote! {
                                    #field_name: orig.#field_name.map(|v| v.into()).unwrap()
                                });
                            }
                        } else {
                            try_from_inits.push(quote! {
                                #field_name: orig.#field_name
                            });
                            from_inits.push(quote! {
                                #field_name: orig.#field_name
                            });
                        }
                    }
                }
            }

            // TODO: the expanded functions can be build outside and the 2 variants for structs and enums can return:
            // * the expanded type definition ( struct or enum )
            // * the implementation of the try_from_inits (only the internal code inside the function)
            // * the implementation of the from_inits (only the internal code inside the function)

            let expanded = quote! {

                #internal_quoted_derives
                #internal_quoted_type_attrs
                #vis struct #internal_name {
                    #(#internal_fields, )*
                }

                impl std::convert::TryFrom<#orig_name> for #internal_name {
                    type Error = String;

                    fn try_from(orig: #orig_name) -> Result<Self, Self::Error> {
                        Ok(#internal_name {
                            #(#try_from_inits, )*
                        })
                    }
                }

                impl From<#internal_name> for #orig_name {
                    fn from(orig: #internal_name) -> Self {
                        #orig_name {
                            #(#from_inits, )*
                        }
                    }
                }
            };

            println!("Generated: \n{expanded}");

            expanded.into()
        }
        //Data::Enum(enum_data) => {
        Data::Enum(DataEnum { variants, .. }) => {
            // Generate Internal enum name

            let mut internal_variants = Vec::new();
            let mut try_from_variants = Vec::new();
            let mut from_variants = Vec::new();

            for variant in variants {
                check_for_forbidden_mandatory_attr(&variant.attrs);

                let variant_ident = &variant.ident;
                let internal_field_attrs = get_internal_field_attrs(&variant.attrs);

                match &variant.fields {
                    Fields::Named(_) => {
                        return syn::Error::new_spanned(
                            variant_ident,
                            "Variants with named fields are not supported.",
                        )
                        .to_compile_error()
                        .into();
                    }
                    Fields::Unnamed(FieldsUnnamed { unnamed, .. }) => {
                        if has_enum_named_attr(&variant.attrs) {
                            if unnamed.len() != 1 {
                                return syn::Error::new_spanned(
                                    variant_ident,
                                    "Variants with 'internal_enum_named' attribute must have exactly one unnamed field",
                                )
                                .to_compile_error().into();
                            }
                            let field = &unnamed[0];
                            check_for_forbidden_mandatory_attr(&field.attrs);

                            // let orig_ty = &field.ty;
                            let new_ty = transform_type(&field.ty, false);

                            // the new named field should start with a lowercase letter
                            let variant_name = variant_ident.to_string();
                            let new_field_name =
                                format_ident!("{}", pascal_to_snake_case(&variant_name));

                            internal_variants.push(quote! {
                                #(#internal_field_attrs )*
                                #variant_ident { #new_field_name: #new_ty }
                            });

                            // Enum::A(String) -> EnumInternal::A { a: String }
                            try_from_variants.push(quote! {
                                #orig_name::#variant_ident( field_0 ) => #internal_name::#variant_ident{ #new_field_name: field_0 } // TODO convert .try_into()? }
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
                                check_for_forbidden_mandatory_attr(&field.attrs);

                                let field_id = format_ident!("field_{i}");
                                let orig_ty = &field.ty;

                                // prepare the try_from and from variants
                                if let Type::Path(tp) = orig_ty {
                                    if is_option(tp) {
                                        return syn::Error::new_spanned(
                                            tp,
                                            "Variants with optional attribute are not supported.",
                                        )
                                        .to_compile_error()
                                        .into();
                                    } else if is_custom_type_path(tp) {
                                        // prepare the enum variant
                                        let new_ty = to_internal_type(tp);
                                        new_variant.push(quote! { #new_ty });

                                        try_fields.push(quote! {
                                            #field_id.try_into()?
                                        });

                                        from_fields.push(quote! {
                                            #field_id.into()
                                        });
                                    } else if let Some(inner) = inner_boxed_type_path(tp)
                                        && is_custom_type_path(&inner)
                                    {
                                        // TODO handle the boxed types
                                        let new_ty = Type::Path(to_internal_type_path(&inner));
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
                                }
                            }

                            internal_variants.push(quote! {
                                #(#internal_field_attrs )*
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
                            #(#internal_field_attrs )*
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

            let expanded = quote! {

                #internal_quoted_derives
                #internal_quoted_type_attrs
                #vis enum #internal_name {
                    #(#internal_variants,)*
            }

            impl std::convert::TryFrom<#orig_name> for #internal_name {
                type Error = String;

                fn try_from(orig: #orig_name) -> Result<Self, Self::Error> {
                    Ok(match orig {
                        #(#try_from_variants),*
                    })
                    // Err("Not implemented yet".into())
                }
            }

            impl From<#internal_name> for #orig_name {
                fn from(original: #internal_name) -> Self {
                    match original {
                        #(#from_variants),*
                    }
                }
            }
            };

            println!("Generated (enum): \n{expanded}");

            expanded.into()
        }
        _ => syn::Error::new_spanned(
            orig_name,
            "Internal derive only supports named structs and enums",
        )
        .to_compile_error()
        .into(),
    }
}

fn pascal_to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c.is_uppercase() {
            let next_is_lower = chars.peek().is_some_and(|&next| next.is_lowercase());

            if !result.is_empty() && (next_is_lower || !result.ends_with('_')) {
                result.push('_');
            }
            result.extend(c.to_lowercase());
        } else {
            result.push(c);
        }
    }

    result
}

/// Extracts all attributes with the `internal_field_attr` meta and returns their tokens for quoting on the internal field.
fn get_internal_field_attrs(attrs: &[Attribute]) -> Vec<proc_macro2::TokenStream> {
    attrs
        .iter()
        .filter_map(|attr| {
            if let Meta::List(meta_list) = &attr.meta
                && meta_list.path.is_ident("internal_field_attr")
            {
                Some(meta_list.tokens.clone())
            } else {
                None
            }
        })
        .collect()
}

fn has_mandatory_attr(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| matches!(&a.meta, Meta::Path(path) if path.is_ident("internal_mandatory")))
}

fn has_enum_named_attr(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| matches!(&a.meta, Meta::Path(path) if path.is_ident("internal_enum_named")))
}

struct DeriveList(Punctuated<Path, Token![,]>);
impl Parse for DeriveList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(DeriveList(Punctuated::parse_terminated(input)?))
    }
}

fn check_for_forbidden_mandatory_attr(attrs: &[Attribute]) {
    if has_mandatory_attr(attrs) {
        panic!("'internal_mandatory' attributes are allowed only on struct fields.");
    }
}

fn is_option(ty: &TypePath) -> bool {
    for segment in &ty.path.segments {
        println!("Checking segment: {}", segment.ident);
    }
    !ty.path.segments.is_empty() && ty.path.segments.last().unwrap().ident == "Option"
}

fn extract_inner(ty: &TypePath) -> Type {
    if let PathArguments::AngleBracketed(generic) = &ty.path.segments.last().unwrap().arguments {
        if generic.args.len() != 1 {
            panic!("Expected exactly one generic argument for G<T>");
        }
        if let Some(syn::GenericArgument::Type(inner_ty)) = generic.args.first() {
            return inner_ty.clone();
        }
    }
    panic!("Expected G<T>");
}

fn is_custom_type(ty: &Type) -> bool {
    if let Type::Path(tp) = ty {
        is_custom_type_path(tp)
    } else {
        false
    }
}

fn is_custom_type_path(tp: &TypePath) -> bool {
    let ident = &tp.path.segments.last().unwrap().ident;
    if ident == "Option" {
        // Recursively check the inner type
        if let Type::Path(tp) = extract_inner(tp) {
            return is_custom_type_path(&tp);
        }
        false
    } else {
        !matches!(
            ident.to_string().as_str(),
            "String"
                | "str"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "f32"
                | "f64"
                | "bool"
                | "Vec"
                | "VecDeque"
                | "HashMap"
                | "HashSet"
                | "BTreeMap"
                | "BTreeSet"
                | "Box"
                | "Option"
        )
    }
}

/// Checks if the given TypePath is a Box<T>
fn is_box_type_path(tp: &TypePath) -> bool {
    tp.path
        .segments
        .last()
        .is_some_and(|seg| seg.ident == "Box")
}

/// Returns the inner TypePath T if the given TypePath is a Box<T>, otherwise None.
fn inner_boxed_type_path(tp: &TypePath) -> Option<TypePath> {
    if is_box_type_path(tp) {
        if let Type::Path(inner) = extract_inner(tp) {
            return Some(inner);
        }
    }
    None
}

fn to_internal_type(ty: &TypePath) -> Type {
    Type::Path(to_internal_type_path(ty))
}

fn to_internal_type_path(tp: &TypePath) -> TypePath {
    let mut new_path = tp.clone();
    let last = new_path.path.segments.last_mut().unwrap();
    last.ident = Ident::new(&format!("{}Internal", last.ident), last.ident.span());
    new_path
}

fn wrap_in_option(inner: Type) -> Type {
    syn::parse_quote! { Option<#inner> }
}

fn transform_type(orig_ty: &Type, mandatory: bool) -> Type {
    match orig_ty {
        Type::Path(tp) if is_option(tp) => {
            let inner = extract_inner(tp);
            if mandatory {
                transform_type(&inner, true)
            } else {
                wrap_in_option(transform_type(&inner, true))
            }
        }
        Type::Path(tp) => {
            let new_type_path = if is_custom_type_path(tp) {
                to_internal_type_path(tp)
            } else {
                tp.clone()
            };

            let new_type_path = if has_generic_args(&new_type_path) {
                // Not a custom type but has generic args - transform them
                transform_type_generic_type(new_type_path)
            } else {
                new_type_path
            };

            Type::Path(new_type_path)
        }
        _ => orig_ty.clone(),
    }
}

fn transform_type_generic_type(mut tp: TypePath) -> TypePath {
    if let Some(last_segment) = tp.path.segments.last_mut() {
        // Recursively transform generic arguments
        if let PathArguments::AngleBracketed(args) = &mut last_segment.arguments {
            for arg in &mut args.args {
                if let GenericArgument::Type(ty) = arg {
                    *ty = transform_type(ty, true);
                }
            }
        }
    }

    tp
}

fn has_generic_args(tp: &TypePath) -> bool {
    if let Some(segment) = tp.path.segments.last() {
        matches!(segment.arguments, PathArguments::AngleBracketed(_))
    } else {
        false
    }
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
    use syn::{
        Attribute, Type,
        parse::{Parse, Parser},
        parse_quote,
    };

    #[test]
    fn test_wrap_in_option_with_simple_type() {
        let inner: Type = parse_quote! { String };
        let wrapped = super::wrap_in_option(inner.clone());
        let expected: Type = parse_quote! { Option<String> };
        assert_eq!(
            quote::quote!(#wrapped).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_wrap_in_option_with_custom_type() {
        let inner: Type = parse_quote! { MyType };
        let wrapped = super::wrap_in_option(inner.clone());
        let expected: Type = parse_quote! { Option<MyType> };
        assert_eq!(
            quote::quote!(#wrapped).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_wrap_in_option_with_generic_type() {
        let inner: Type = parse_quote! { Vec<u32> };
        let wrapped = super::wrap_in_option(inner.clone());
        let expected: Type = parse_quote! { Option<Vec<u32>> };
        assert_eq!(
            quote::quote!(#wrapped).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_is_custom_type_with_primitive() {
        let ty: Type = parse_quote! { u32 };
        assert!(!super::is_custom_type(&ty));

        let ty: Type = parse_quote! { bool };
        assert!(!super::is_custom_type(&ty));

        let ty: Type = parse_quote! { String };
        assert!(!super::is_custom_type(&ty));
    }

    #[test]
    fn test_is_custom_type_with_std_collections() {
        let ty: Type = parse_quote! { Vec<u8> };
        assert!(!super::is_custom_type(&ty));

        let ty: Type = parse_quote! { HashMap<String, u32> };
        assert!(!super::is_custom_type(&ty));

        let ty: Type = parse_quote! { BTreeSet<i32> };
        assert!(!super::is_custom_type(&ty));
    }

    #[test]
    fn test_is_custom_type_with_custom_type() {
        let ty: Type = parse_quote! { MyStruct };
        assert!(super::is_custom_type(&ty));

        let ty: Type = parse_quote! { CustomType123 };
        assert!(super::is_custom_type(&ty));
    }

    #[test]
    fn test_is_custom_type_with_boxed_custom_type() {
        let ty: Type = parse_quote! { Box<MyStruct> };
        assert!(!super::is_custom_type(&ty));
    }

    #[test]
    fn test_is_custom_type_with_nested_path() {
        let ty: Type = parse_quote! { my_mod::MyStruct };
        assert!(super::is_custom_type(&ty));
    }

    #[test]
    fn test_is_custom_type_with_custom_generic_type() {
        let ty: Type = parse_quote! { MyStruct<T> };
        assert!(super::is_custom_type(&ty));

        let ty: Type = parse_quote! { another_mod::CustomType<A, B> };
        assert!(super::is_custom_type(&ty));
    }

    #[test]
    fn test_is_custom_type_with_option() {
        let ty: Type = parse_quote! { Option<i32> };
        assert!(!super::is_custom_type(&ty));

        let ty: Type = parse_quote! { Option<MyStruct> };
        assert!(super::is_custom_type(&ty));
    }

    // #[test]
    // fn test_to_internal_type_simple() {
    //     let tp: syn::TypePath = parse_quote! { MyStruct };
    //     let result = super::to_internal_type(&tp);
    //     let expected: Type = parse_quote! { MyStructInternal };
    //     assert_eq!(
    //         quote::quote!(#result).to_string(),
    //         quote::quote!(#expected).to_string()
    //     );
    // }

    // #[test]
    // fn test_to_internal_type_with_module_path() {
    //     let tp: syn::TypePath = parse_quote! { my_mod::MyStruct };
    //     let result = super::to_internal_type(&tp);
    //     let expected: Type = parse_quote! { my_mod::MyStructInternal };
    //     assert_eq!(
    //         quote::quote!(#result).to_string(),
    //         quote::quote!(#expected).to_string()
    //     );
    // }

    // #[test]
    // fn test_to_internal_type_with_generic() {
    //     let tp: syn::TypePath = parse_quote! { MyStruct<T, U> };
    //     let result = super::to_internal_type(&tp);
    //     let expected: Type = parse_quote! { MyStructInternal<T, U> };
    //     assert_eq!(
    //         quote::quote!(#result).to_string(),
    //         quote::quote!(#expected).to_string()
    //     );
    // }

    // #[test]
    // fn test_to_internal_type_with_nested_module_and_generic() {
    //     let tp: syn::TypePath = parse_quote! { outer::inner::CustomType<A, B> };
    //     let result = super::to_internal_type(&tp);
    //     let expected: Type = parse_quote! { outer::inner::CustomTypeInternal<A, B> };
    //     assert_eq!(
    //         quote::quote!(#result).to_string(),
    //         quote::quote!(#expected).to_string()
    //     );
    // }

    #[test]
    fn test_is_option_with_option_type() {
        let tp: syn::TypePath = parse_quote! { Option<u32> };
        assert!(super::is_option(&tp));

        let tp: syn::TypePath = parse_quote! { Option<String> };
        assert!(super::is_option(&tp));

        let tp: syn::TypePath = parse_quote! { Option<MyStruct> };
        assert!(super::is_option(&tp));

        let tp: syn::TypePath =
            parse_quote! { Option<MyStruct<MyOtherStruct<WithAnother<StructInside>>>> };
        assert!(super::is_option(&tp));
    }

    #[test]
    fn test_is_option_with_non_option_type() {
        let tp: syn::TypePath = parse_quote! { Result<u32, String> };
        assert!(!super::is_option(&tp));

        let tp: syn::TypePath = parse_quote! { Vec<u32> };
        assert!(!super::is_option(&tp));

        let tp: syn::TypePath = parse_quote! { MyStruct };
        assert!(!super::is_option(&tp));
    }

    #[test]
    fn test_is_option_with_nested_option() {
        let tp: syn::TypePath = parse_quote! { Option<Option<u32>> };
        assert!(super::is_option(&tp));
    }

    #[test]
    fn test_is_option_with_option_of_vec() {
        let tp: syn::TypePath = parse_quote! { Option<Vec<u32>> };
        assert!(super::is_option(&tp));
    }

    #[test]
    fn test_transform_type_option_mandatory_primitive() {
        let orig_ty: Type = parse_quote! { Option<u32> };
        let result = super::transform_type(&orig_ty, true);
        let expected: Type = parse_quote! { u32 };
        assert_eq!(
            quote::quote!(#result).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_transform_type_option_non_mandatory_primitive() {
        let orig_ty: Type = parse_quote! { Option<u32> };
        let result = super::transform_type(&orig_ty, false);
        let expected: Type = parse_quote! { Option<u32> };
        assert_eq!(
            quote::quote!(#result).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_transform_type_option_mandatory_custom() {
        let orig_ty: Type = parse_quote! { Option<MyStruct> };
        let result = super::transform_type(&orig_ty, true);
        let expected: Type = parse_quote! { MyStructInternal };
        assert_eq!(
            quote::quote!(#result).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_transform_type_option_non_mandatory_custom() {
        let orig_ty: Type = parse_quote! { Option<MyStruct> };
        let result = super::transform_type(&orig_ty, false);
        let expected: Type = parse_quote! { Option<MyStructInternal> };
        assert_eq!(
            quote::quote!(#result).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_transform_type_plain_primitive() {
        let orig_ty: Type = parse_quote! { u64 };
        let result = super::transform_type(&orig_ty, true);
        let expected: Type = parse_quote! { u64 };
        assert_eq!(
            quote::quote!(#result).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_transform_type_plain_custom() {
        let orig_ty: Type = parse_quote! { MyStruct };
        let result = super::transform_type(&orig_ty, true);
        let expected: Type = parse_quote! { MyStructInternal };
        assert_eq!(
            quote::quote!(#result).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_transform_type_plain_custom_with_generic() {
        let orig_ty: Type = parse_quote! { MyStruct<i32, String> };
        let result = super::transform_type(&orig_ty, true);
        let expected: Type = parse_quote! { MyStructInternal<i32, String> };
        assert_eq!(
            quote::quote!(#result).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_transform_type_option_custom_with_generic_non_mandatory() {
        let orig_ty: Type = parse_quote! { Option<MyStruct<i32, String>> };
        let result = super::transform_type(&orig_ty, false);
        let expected: Type = parse_quote! { Option<MyStructInternal<i32, String>> };
        assert_eq!(
            quote::quote!(#result).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_transform_type_option_custom_with_generic_mandatory() {
        let orig_ty: Type = parse_quote! { Option<MyStruct<i32, String>> };
        let result = super::transform_type(&orig_ty, true);
        let expected: Type = parse_quote! { MyStructInternal<i32, String> };
        assert_eq!(
            quote::quote!(#result).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_transform_type_plain_std_collection() {
        let orig_ty: Type = parse_quote! { Vec<u8> };
        let result = super::transform_type(&orig_ty, true);
        let expected: Type = parse_quote! { Vec<u8> };
        assert_eq!(
            quote::quote!(#result).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_transform_type_boxed_custom_type() {
        let orig_ty: Type = parse_quote! { Box<MyStruct> };
        let result = super::transform_type(&orig_ty, true);
        let expected: Type = parse_quote! { Box<MyStructInternal> };
        assert_eq!(
            quote::quote!(#result).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_transform_type_generic_custom_with_generic_type() {
        let orig_ty: Type = parse_quote! { MyGeneric<MyStruct, MyType, String> };
        let result = super::transform_type(&orig_ty, true);
        let expected: Type =
            parse_quote! { MyGenericInternal<MyStructInternal, MyTypeInternal, String> };
        assert_eq!(
            quote::quote!(#result).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_has_mandatory_attr_with_mandatory() {
        let attrs: Vec<Attribute> = vec![parse_quote!(#[internal_mandatory])];
        assert!(super::has_mandatory_attr(&attrs));
    }

    #[test]
    fn test_has_mandatory_attr_without_mandatory() {
        let attrs: Vec<Attribute> = vec![parse_quote!(#[serde(rename = "foo")])];
        assert!(!super::has_mandatory_attr(&attrs));
    }

    #[test]
    fn test_has_mandatory_attr_with_multiple_attrs_including_mandatory() {
        let attrs: Vec<Attribute> = vec![
            parse_quote!(#[serde(rename = "foo")]),
            parse_quote!(#[internal_mandatory]),
            parse_quote!(#[doc = "Some doc"]),
        ];
        assert!(super::has_mandatory_attr(&attrs));
    }

    #[test]
    fn test_has_mandatory_attr_with_empty_attrs() {
        let attrs: Vec<syn::Attribute> = vec![];
        assert!(!super::has_mandatory_attr(&attrs));
    }

    #[test]
    fn test_extract_inner_with_option_primitive() {
        let tp: syn::TypePath = parse_quote! { Option<u32> };
        let inner = super::extract_inner(&tp);
        let expected: Type = parse_quote! { u32 };
        assert_eq!(
            quote::quote!(#inner).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_extract_inner_with_option_custom_type() {
        let tp: syn::TypePath = parse_quote! { Option<MyStruct> };
        let inner = super::extract_inner(&tp);
        let expected: Type = parse_quote! { MyStruct };
        assert_eq!(
            quote::quote!(#inner).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_extract_inner_with_option_generic_type() {
        let tp: syn::TypePath = parse_quote! { Option<Vec<u8>> };
        let inner = super::extract_inner(&tp);
        let expected: Type = parse_quote! { Vec<u8> };
        assert_eq!(
            quote::quote!(#inner).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_extract_inner_with_option_nested_custom_type() {
        let tp: syn::TypePath = parse_quote! { Option<my_mod::MyStruct> };
        let inner = super::extract_inner(&tp);
        let expected: Type = parse_quote! { my_mod::MyStruct };
        assert_eq!(
            quote::quote!(#inner).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_extract_inner_with_option_of_option() {
        let tp: syn::TypePath = parse_quote! { Option<Option<u32>> };
        let inner = super::extract_inner(&tp);
        let expected: Type = parse_quote! { Option<u32> };
        assert_eq!(
            quote::quote!(#inner).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_derive_list_parse_single_path() {
        let input = quote::quote! { Clone };
        let parser = super::DeriveList::parse;
        let result = parser.parse2(input).unwrap();
        assert_eq!(result.0.len(), 1);
        assert_eq!(result.0.first().unwrap().segments[0].ident, "Clone");
    }

    #[test]
    fn test_derive_list_parse_multiple_paths() {
        let input = quote::quote! { Clone, Debug, PartialEq };
        let parser = super::DeriveList::parse;
        let result = parser.parse2(input).unwrap();
        let idents: Vec<_> = result
            .0
            .iter()
            .map(|p| p.segments[0].ident.to_string())
            .collect();
        assert_eq!(idents, vec!["Clone", "Debug", "PartialEq"]);
    }

    #[test]
    fn test_derive_list_parse_with_module_paths() {
        let input = quote::quote! { std::fmt::Debug, my_mod::CustomTrait };
        let parser = super::DeriveList::parse;
        let result = parser.parse2(input).unwrap();
        let idents: Vec<_> = result
            .0
            .iter()
            .map(|p| p.segments.last().unwrap().ident.to_string())
            .collect();
        assert_eq!(idents, vec!["Debug", "CustomTrait"]);
    }

    #[test]
    fn test_derive_list_parse_empty() {
        let input = quote::quote! {};
        let parser = super::DeriveList::parse;
        let result = parser.parse2(input).unwrap();
        assert_eq!(result.0.len(), 0);
    }

    #[test]
    fn test_derive_list_parse_trailing_comma() {
        let input = quote::quote! { Clone, Debug, };
        let parser = super::DeriveList::parse;
        let result = parser.parse2(input).unwrap();
        let idents: Vec<_> = result
            .0
            .iter()
            .map(|p| p.segments[0].ident.to_string())
            .collect();
        assert_eq!(idents, vec!["Clone", "Debug"]);
    }

    // #[test]
    // fn test_derive_internal_simple_struct() {
    //     let input: DeriveInput = parse_quote! {
    //         pub struct Foo {
    //             #[mandatory]
    //             a: u32,
    //             b: Option<String>,
    //         }
    //     };
    //     let tokens = quote::quote! { #input };
    //     let output: proc_macro2::TokenStream =
    //         super::derive_internal(proc_macro::TokenStream::from(tokens)).into();

    //     let output_str = output.to_string();
    //     assert!(output_str.contains("struct FooInternal"));
    //     assert!(output_str.contains("a : u32"));
    //     assert!(output_str.contains("b : Option < String >"));
    //     assert!(output_str.contains("impl std :: convert :: TryFrom < Foo > for FooInternal"));
    //     assert!(output_str.contains("impl From < FooInternal > for Foo"));
    // }

    // #[test]
    // fn test_derive_internal_with_custom_type_and_option() {
    //     let input: DeriveInput = parse_quote! {
    //         struct Bar {
    //             #[mandatory]
    //             x: Option<MyType>,
    //             y: u32,
    //         }
    //     };
    //     let tokens = quote! { #input };
    //     let output: proc_macro2::TokenStream =
    //         super::derive_internal(proc_macro::TokenStream::from(tokens)).into();

    //     let output_str = output.to_string();
    //     assert!(output_str.contains("struct BarInternal"));
    //     assert!(output_str.contains("x : MyTypeInternal"));
    //     assert!(output_str.contains("y : u32"));
    // }

    // #[test]
    // fn test_derive_internal_with_internal_derive_attr() {
    //     let input: DeriveInput = parse_quote! {
    //         #[internal_derive(Clone, Debug)]
    //         pub struct Baz {
    //             #[mandatory]
    //             foo: u8,
    //         }
    //     };
    //     let tokens = quote! { #input };
    //     let output: proc_macro2::TokenStream =
    //         super::derive_internal(proc_macro::TokenStream::from(tokens)).into();

    //     let output_str = output.to_string();
    //     assert!(output_str.contains("# [ derive ( Clone , Debug ) ]"));
    //     assert!(output_str.contains("struct BazInternal"));
    // }

    // #[test]
    // fn test_derive_internal_rejects_unnamed_struct() {
    //     let input: DeriveInput = parse_quote! {
    //         struct TupleStruct(u32, String);
    //     };
    //     let tokens = quote! { #input };
    //     let output: proc_macro2::TokenStream =
    //         super::derive_internal(proc_macro::TokenStream::from(tokens)).into();

    //     let output_str = output.to_string();
    //     assert!(output_str.contains("Internal derive only supports named structs"));
    // }

    // #[test]
    // fn test_derive_internal_with_nested_custom_types() {
    //     let input: DeriveInput = parse_quote! {
    //         struct Outer {
    //             #[mandatory]
    //             inner: Option<my_mod::InnerType>,
    //         }
    //     };
    //     let tokens = quote! { #input };
    //     let output: proc_macro2::TokenStream =
    //         super::derive_internal(proc_macro::TokenStream::from(tokens)).into();

    //     let output_str = output.to_string();
    //     assert!(output_str.contains("inner : my_mod :: InnerTypeInternal"));
    // }
}
