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
use quote::{ToTokens, format_ident, quote};
use syn::{
    Attribute, Expr, GenericArgument, Ident, Lit, Meta, MetaNameValue, Path, PathArguments, Token,
    Type, TypePath,
    parse::{Parse, ParseStream},
    parse_quote,
    punctuated::Punctuated,
};

pub struct DerivedInternal {
    pub obj: TokenStream,
    pub try_from_impl: TokenStream,
    pub from_impl: TokenStream,
}

impl ToTokens for DerivedInternal {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let obj = &self.obj;
        let try_from_impl = &self.try_from_impl;
        let from_impl = &self.from_impl;
        tokens.extend(quote! {
            #obj
            #try_from_impl
            #from_impl
        });
    }
}

pub fn has_skip_try_from(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| matches!(&a.meta, Meta::Path(path) if path.is_ident("internal_skip_try_from")))
}

pub fn get_internal_type_attrs(attrs: &[Attribute]) -> Vec<proc_macro2::TokenStream> {
    let mut internal_type_attrs = Vec::new();
    for attr in attrs {
        if let Meta::List(meta_list) = &attr.meta {
            if meta_list.path.is_ident("internal_derive") {
                let parsed = syn::parse2::<AttributeList>(meta_list.tokens.clone()).unwrap();
                let derive_list: Vec<Path> = parsed.0.into_iter().collect();
                internal_type_attrs.push(quote! { #[derive(#(#derive_list),*)] });
            } else if meta_list.path.is_ident("internal_type_attr") {
                internal_type_attrs.push(meta_list.tokens.clone());
            }
        }
    }

    internal_type_attrs
}

pub fn pascal_to_snake_case(s: &str) -> String {
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

pub fn get_prost_map_enum_value_type(attrs: &[Attribute]) -> Option<TypePath> {
    for attr in attrs {
        // Check if this is a #[prost(...)] attribute
        if !attr.path().is_ident("prost") {
            continue;
        }

        // Parse the attribute's arguments as a list of Meta items
        let Ok(nested) = attr
            .parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
        else {
            continue;
        };

        for meta in nested {
            // Look for map = "key_type, enumeration(ReadWriteEnum)"
            if let Meta::NameValue(MetaNameValue { path, value, .. }) = meta
                && path.is_ident("map")
                && let Expr::Lit(expr_lit) = value
                && let Lit::Str(lit_str) = expr_lit.lit
                && let map_value = lit_str.value()
                && let parts = map_value
                    .split(',')
                    .map(|s| s.trim())
                    .collect::<Vec<&str>>()
                && parts.len() == 2
                && let Some(part) = parts.last()
                && part.starts_with("enumeration")
                && let enum_parts = part
                    .split_terminator(&['(', ')'][..])
                    .map(|s| s.trim())
                    .collect::<Vec<&str>>()
                && enum_parts.len() == 2
            {
                let enum_name = format_ident!("{}", enum_parts[1]);

                return Some(parse_quote! { #enum_name });
            }
        }
    }

    None
}

pub fn get_prost_enum_type(attrs: &[Attribute]) -> Option<TypePath> {
    for attr in attrs {
        // Check if this is a #[prost(...)] attribute
        if !attr.path().is_ident("prost") {
            continue;
        }

        // Parse the attribute's arguments as a list of Meta items
        let Ok(nested) = attr
            .parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
        else {
            continue;
        };

        for meta in nested {
            // Look for enumeration = "ReadWriteEnum"
            if let Meta::NameValue(MetaNameValue { path, value, .. }) = meta {
                if path.is_ident("enumeration") {
                    if let Expr::Lit(expr_lit) = value {
                        if let Lit::Str(lit_str) = expr_lit.lit {
                            let enum_name = lit_str.value();
                            // Parse the string as a Rust path
                            // if it does not work, it's not what we expect
                            if let Ok(tp) = syn::parse_str::<TypePath>(&enum_name) {
                                return Some(tp);
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

/// Extracts all attributes with the `internal_field_attr` meta and returns their tokens for quoting on the internal field.
pub fn get_internal_field_attrs(attrs: &[Attribute]) -> Vec<proc_macro2::TokenStream> {
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

pub fn has_mandatory_attr(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| matches!(&a.meta, Meta::Path(path) if path.is_ident("internal_mandatory")))
}

pub fn has_enum_named_attr(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| matches!(&a.meta, Meta::Path(path) if path.is_ident("internal_enum_named")))
}

pub struct AttributeList(Punctuated<Path, Token![,]>);
impl Parse for AttributeList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(AttributeList(Punctuated::parse_terminated(input)?))
    }
}

// Variant and Field do not implement a common train for attributes
// so we need to take both the target and the attrs as parameters
pub fn check_for_forbidden_mandatory_attr(
    target: &impl ToTokens,
    attrs: &[Attribute],
) -> syn::Result<()> {
    if has_mandatory_attr(attrs) {
        return Err(syn::Error::new_spanned(
            target,
            "'internal_mandatory' attributes are allowed only on struct fields.",
        ));
    }
    Ok(())
}

pub fn is_option_type_path(tp: &TypePath) -> bool {
    !tp.path.segments.is_empty() && tp.path.segments.last().unwrap().ident == "Option"
}

pub fn extract_inner(ty: &TypePath) -> TypePath {
    if let PathArguments::AngleBracketed(generic) = &ty.path.segments.last().unwrap().arguments {
        if generic.args.len() != 1 {
            panic!("Expected exactly one generic argument for G<T>");
        }
        if let Some(syn::GenericArgument::Type(Type::Path(inner_tp))) = generic.args.first() {
            return inner_tp.clone();
        }
    }
    panic!("Expected G<T>");
}

pub fn is_custom_type_path(tp: &TypePath) -> bool {
    let ident = &tp.path.segments.last().unwrap().ident;
    if ident == "Option" {
        // Recursively check the inner type
        is_custom_type_path(&extract_inner(tp))
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
pub fn is_box_type_path(tp: &TypePath) -> bool {
    tp.path
        .segments
        .last()
        .is_some_and(|seg| seg.ident == "Box")
}

/// Checks if the given TypePath is a Box<T>
pub fn is_vec_type_path(tp: &TypePath) -> bool {
    tp.path
        .segments
        .last()
        .is_some_and(|seg| seg.ident == "Vec")
}

/// Checks if the given TypePath is a Box<T>
pub fn is_hashmap_type_path(tp: &TypePath) -> bool {
    tp.path
        .segments
        .last()
        .is_some_and(|seg| seg.ident == "HashMap")
}

/// Returns the inner TypePath T if the given TypePath is a Box<T>, otherwise None.
pub fn inner_boxed_type_path(tp: &TypePath) -> Option<TypePath> {
    if is_box_type_path(tp) {
        return Some(extract_inner(tp));
    }
    None
}

/// Returns the inner TypePath T if the given TypePath is a Vec<T>, otherwise None.
pub fn inner_vec_type_path(tp: &TypePath) -> Option<TypePath> {
    if is_vec_type_path(tp) {
        return Some(extract_inner(tp));
    }
    None
}

/// Returns the inner TypePath T if the given TypePath is a Vec<T>, otherwise None.
pub fn inner_hashmap_type_path(tp: &TypePath) -> Option<(TypePath, TypePath)> {
    if is_hashmap_type_path(tp) {
        if let PathArguments::AngleBracketed(generic) = &tp.path.segments.last().unwrap().arguments
        {
            if generic.args.len() == 2 {
                if let (Some(GenericArgument::Type(key_ty)), Some(GenericArgument::Type(val_ty))) =
                    (generic.args.first(), generic.args.last())
                {
                    if let (Type::Path(key_tp), Type::Path(val_tp)) = (key_ty, val_ty) {
                        return Some((key_tp.clone(), val_tp.clone()));
                    }
                }
            }
        }
    }
    None
}

pub fn to_internal_type(tp: &TypePath) -> Type {
    let mut new_path = tp.clone();
    let last = new_path.path.segments.last_mut().unwrap();
    last.ident = Ident::new(&format!("{}Internal", last.ident), last.ident.span());
    Type::Path(new_path)
}

pub fn wrap_in_option(inner: Type) -> Type {
    syn::parse_quote! { Option<#inner> }
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
        let parser = super::AttributeList::parse;
        let result = parser.parse2(input).unwrap();
        assert_eq!(result.0.len(), 1);
        assert_eq!(result.0.first().unwrap().segments[0].ident, "Clone");
    }

    #[test]
    fn test_derive_list_parse_multiple_paths() {
        let input = quote::quote! { Clone, Debug, PartialEq };
        let parser = super::AttributeList::parse;
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
        let parser = super::AttributeList::parse;
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
        let parser = super::AttributeList::parse;
        let result = parser.parse2(input).unwrap();
        assert_eq!(result.0.len(), 0);
    }

    #[test]
    fn test_derive_list_parse_trailing_comma() {
        let input = quote::quote! { Clone, Debug, };
        let parser = super::AttributeList::parse;
        let result = parser.parse2(input).unwrap();
        let idents: Vec<_> = result
            .0
            .iter()
            .map(|p| p.segments[0].ident.to_string())
            .collect();
        assert_eq!(idents, vec!["Clone", "Debug"]);
    }
}
