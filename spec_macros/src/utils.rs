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

pub struct DerivedSpec {
    pub obj: TokenStream,
    pub try_from_impl: TokenStream,
    pub from_impl: TokenStream,
}

impl ToTokens for DerivedSpec {
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

pub fn get_doc_attrs(attrs: &[Attribute]) -> Vec<proc_macro2::TokenStream> {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .map(Attribute::to_token_stream)
        .collect()
}

pub fn get_spec_type_attrs(attrs: &[Attribute]) -> Vec<proc_macro2::TokenStream> {
    let mut spec_type_attrs = Vec::new();
    for attr in attrs {
        if let Meta::List(meta_list) = &attr.meta {
            if meta_list.path.is_ident("spec_derive") {
                let parsed = syn::parse2::<AttributeList>(meta_list.tokens.clone()).unwrap();
                let derive_list: Vec<Path> = parsed.0.into_iter().collect();
                spec_type_attrs.push(quote! { #[derive(#(#derive_list),*)] });
            } else if meta_list.path.is_ident("spec_type_attr") {
                spec_type_attrs.push(meta_list.tokens.clone());
            }
        }
    }

    spec_type_attrs
}

pub fn pascal_to_snake_case(input: &str) -> String {
    let mut result = String::new();

    let chars: Vec<char> = input.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            // Insert underscore if:
            // - not first char
            // - previous char was not uppercase (start of new word)
            // - next char is lowercase (end of acronym)
            let next_is_lower = chars.get(i + 1).map(|n| n.is_lowercase()).unwrap_or(false);
            let prev_is_lower = i > 0 && chars[i - 1].is_lowercase();
            if i > 0 && (prev_is_lower || next_is_lower) {
                result.push('_');
            }
        } else if c == '_' {
            // skip extra underscores
            continue;
        }
        result.extend(c.to_lowercase());
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

/// Extracts all attributes with the `spec_field_attr` meta and returns their tokens for quoting on the spec field.
pub fn get_spec_field_attrs(attrs: &[Attribute]) -> Vec<proc_macro2::TokenStream> {
    attrs
        .iter()
        .filter_map(|attr| {
            if let Meta::List(meta_list) = &attr.meta
                && meta_list.path.is_ident("spec_field_attr")
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
        .any(|a| matches!(&a.meta, Meta::Path(path) if path.is_ident("spec_mandatory")))
}

pub fn has_default_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| match &a.meta {
        Meta::Path(path) => path.is_ident("spec_default"),
        Meta::List(meta_list) => meta_list.path.is_ident("spec_default"),
        _ => false,
    })
}

fn get_default_attr_value(attrs: &[Attribute]) -> Option<proc_macro2::TokenStream> {
    for a in attrs {
        match &a.meta {
            Meta::List(meta_list) if meta_list.path.is_ident("spec_default") => {
                let nested = &meta_list.tokens;
                let parsed: Result<Expr, _> = syn::parse2(nested.clone());
                if let Ok(Expr::Lit(expr_lit)) = parsed {
                    return Some(expr_lit.to_token_stream());
                }
            }
            _ => {}
        }
    }
    None
}

pub fn get_option_handling(attrs: &[Attribute], field_name: &Ident) -> Option<TokenStream> {
    if has_mandatory_attr(attrs) {
        let missing_field_msg = format!("Missing field '{field_name}'");
        Some(quote! {
            .ok_or(#missing_field_msg)?
        })
    } else if has_default_attr(attrs) {
        if let Some(default_value) = get_default_attr_value(attrs) {
            Some(quote! {
                .unwrap_or(#default_value)
            })
        } else {
            Some(quote! {
                .unwrap_or_default()
            })
        }
    } else {
        None
    }
}

pub fn has_enum_named_attr(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| matches!(&a.meta, Meta::Path(path) if path.is_ident("spec_enum_named")))
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
            "'spec_mandatory' attributes are allowed only on struct fields.",
        ));
    }
    Ok(())
}

pub fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        is_option_type_path(type_path)
    } else {
        false
    }
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

const SPEC_MACRO_SUFFIX: &str = "Spec";

pub fn to_spec_ident(ident: &Ident) -> Ident {
    format_ident!("{}{}", ident, SPEC_MACRO_SUFFIX)
}

pub fn to_spec_type(tp: &TypePath) -> Type {
    let mut new_path = tp.clone();
    let last = new_path.path.segments.last_mut().unwrap();
    last.ident = to_spec_ident(&last.ident);
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
    use proc_macro2::TokenStream;
    use quote::{ToTokens, quote};
    use syn::{
        Attribute, Ident, Type, TypePath,
        parse::{Parse, Parser},
        parse_quote,
    };

    #[test]
    fn test_to_tokens_derived_spec() {
        let derived_spec = super::DerivedSpec {
            obj: quote! { struct MyStruct; },
            try_from_impl: quote! { impl TryFrom<()> for MyStruct { type Error = (); fn try_from(_: ()) -> Result<Self, Self::Error> { Ok(MyStruct) } } },
            from_impl: quote! { impl From<MyStruct> for () { fn from(_: MyStruct) -> Self { () } } },
        };

        let mut tokens = TokenStream::new();
        derived_spec.to_tokens(&mut tokens);

        let expected = quote! {
            struct MyStruct;
            impl TryFrom<()> for MyStruct { type Error = (); fn try_from(_: ()) -> Result<Self, Self::Error> { Ok(MyStruct) } }
            impl From<MyStruct> for () { fn from(_: MyStruct) -> Self { () } }
        };

        assert_eq!(tokens.to_string(), expected.to_string());
    }

    #[test]
    fn test_get_spec_type_attrs() {
        let attrs: Vec<Attribute> = vec![
            parse_quote!(#[spec_derive(Debug, Clone)]),
            parse_quote!(#[spec_type_attr(serde(rename_all = "snake_case"))]),
            parse_quote!(#[doc = "Some documentation"]),
        ];

        let spec_attrs = super::get_spec_type_attrs(&attrs);
        assert_eq!(spec_attrs.len(), 2);
        assert_eq!(
            spec_attrs[0].to_string(),
            quote::quote!( #[derive( Debug , Clone )] ).to_string()
        );
        assert_eq!(
            spec_attrs[1].to_string(),
            quote::quote!(serde(rename_all = "snake_case")).to_string()
        );
    }

    #[test]
    fn test_pascal_to_snake_case() {
        assert_eq!(super::pascal_to_snake_case("MyType"), "my_type");
        assert_eq!(super::pascal_to_snake_case("XMLParser"), "xml_parser");
        assert_eq!(super::pascal_to_snake_case("SimpleTest"), "simple_test");
        assert_eq!(super::pascal_to_snake_case("A"), "a");
        assert_eq!(super::pascal_to_snake_case("ThisIsATest"), "this_is_a_test");
        assert_eq!(
            super::pascal_to_snake_case("Already_Snake_Case"),
            "already_snake_case"
        );
        assert_eq!(
            super::pascal_to_snake_case("MixedCASEExample"),
            "mixed_case_example"
        );
        assert_eq!(
            super::pascal_to_snake_case("JSONToXMLConverter"),
            "json_to_xml_converter"
        );
        assert_eq!(super::pascal_to_snake_case("EdgeCASEX"), "edge_casex");
        assert_eq!(super::pascal_to_snake_case(""), "");
    }

    #[test]
    fn test_get_prost_map_enum_value_type() {
        let attrs: Vec<Attribute> = vec![
            parse_quote!(#[prost(map = "string, enumeration(ReadWriteEnum)")]),
            parse_quote!(#[serde(rename = "foo")]),
        ];

        let enum_type = super::get_prost_map_enum_value_type(&attrs).unwrap();
        let expected: TypePath = parse_quote! { ReadWriteEnum };
        assert_eq!(
            quote::quote!(#enum_type).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_get_prost_enum_type() {
        let attrs: Vec<Attribute> = vec![
            parse_quote!(#[prost(enumeration = "ReadWriteEnum")]),
            parse_quote!(#[serde(rename = "foo")]),
        ];

        let enum_type = super::get_prost_enum_type(&attrs).unwrap();
        let expected: TypePath = parse_quote! { ReadWriteEnum };
        assert_eq!(
            quote::quote!(#enum_type).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_get_spec_field_attrs() {
        let attrs: Vec<Attribute> = vec![
            parse_quote!(#[spec_field_attr(serde(rename = "foo"))]),
            parse_quote!(#[doc = "Some documentation"]),
            parse_quote!(#[spec_field_attr(another_attr)]),
        ];

        let spec_attrs = super::get_spec_field_attrs(&attrs);
        assert_eq!(spec_attrs.len(), 2);
        assert_eq!(
            spec_attrs[0].to_string(),
            quote::quote!(serde(rename = "foo")).to_string()
        );
        assert_eq!(
            spec_attrs[1].to_string(),
            quote::quote!(another_attr).to_string()
        );
    }

    #[test]
    fn test_has_enum_named_attr() {
        let attrs_with_enum_named: Vec<Attribute> = vec![parse_quote!(#[spec_enum_named])];
        let attrs_without_enum_named: Vec<Attribute> = vec![parse_quote!(#[serde(rename = "foo")])];

        assert!(super::has_enum_named_attr(&attrs_with_enum_named));
        assert!(!super::has_enum_named_attr(&attrs_without_enum_named));
    }

    #[test]
    fn test_check_for_forbidden_mandatory_attr() {
        let attrs_with_mandatory: Vec<Attribute> = vec![parse_quote!(#[spec_mandatory])];
        let attrs_without_mandatory: Vec<Attribute> = vec![parse_quote!(#[serde(rename = "foo")])];

        let target: TypePath = parse_quote! { MyCustomType };

        // Test with mandatory attribute
        let result = super::check_for_forbidden_mandatory_attr(&target, &attrs_with_mandatory);
        assert!(result.is_err());

        // Test without mandatory attribute
        let result = super::check_for_forbidden_mandatory_attr(&target, &attrs_without_mandatory);
        assert!(result.is_ok());
    }

    #[test]
    fn test_is_option_type() {
        let ty_option: Type = parse_quote! { Option<u32> };
        assert!(super::is_option_type(&ty_option));

        let ty_non_option: Type = parse_quote! { u32 };
        assert!(!super::is_option_type(&ty_non_option));
    }

    #[test]
    fn test_is_option_type_path() {
        let tp_option: TypePath = parse_quote! { Option<u32> };
        assert!(super::is_option_type_path(&tp_option));

        let tp_non_option: TypePath = parse_quote! { u32 };
        assert!(!super::is_option_type_path(&tp_non_option));
    }

    #[test]
    fn test_is_custom_type_path() {
        let tp_custom: TypePath = parse_quote! { MyCustomType };
        assert!(super::is_custom_type_path(&tp_custom));

        let tp_option_custom: TypePath = parse_quote! { Option<MyCustomType> };
        assert!(super::is_custom_type_path(&tp_option_custom));

        let tp_primitive: TypePath = parse_quote! { u32 };
        assert!(!super::is_custom_type_path(&tp_primitive));

        let tp_option_primitive: TypePath = parse_quote! { Option<u32> };
        assert!(!super::is_custom_type_path(&tp_option_primitive));

        let tp_string: TypePath = parse_quote! { String };
        assert!(!super::is_custom_type_path(&tp_string));

        let tp_option_string: TypePath = parse_quote! { Option<String> };
        assert!(!super::is_custom_type_path(&tp_option_string));

        let tp_vec: TypePath = parse_quote! { Vec<u32> };
        assert!(!super::is_custom_type_path(&tp_vec));

        let tp_option_vec: TypePath = parse_quote! { Option<Vec<u32>> };
        assert!(!super::is_custom_type_path(&tp_option_vec));

        let tp_hashmap: TypePath = parse_quote! { HashMap<String, u32> };
        assert!(!super::is_custom_type_path(&tp_hashmap));

        let tp_option_hashmap: TypePath = parse_quote! { Option<HashMap<String, u32>> };
        assert!(!super::is_custom_type_path(&tp_option_hashmap));

        let tp_box: TypePath = parse_quote! { Box<u32> };
        assert!(!super::is_custom_type_path(&tp_box));

        let tp_option_box: TypePath = parse_quote! { Option<Box<u32>> };
        assert!(!super::is_custom_type_path(&tp_option_box));
    }

    #[test]
    fn test_is_box_type_path() {
        let tp: TypePath = parse_quote! { Box<u32> };
        assert!(super::is_box_type_path(&tp));

        let tp_non: TypePath = parse_quote! { Vec<u32> };
        assert!(!super::is_box_type_path(&tp_non));
    }

    #[test]
    fn test_is_vec_type_path() {
        let tp: TypePath = parse_quote! { Vec<u32> };
        assert!(super::is_vec_type_path(&tp));

        let tp_non: TypePath = parse_quote! { HashMap<String, u32> };
        assert!(!super::is_vec_type_path(&tp_non));
    }

    #[test]
    fn test_is_hashmap_type_path() {
        let tp: TypePath = parse_quote! { HashMap<String, u32> };
        assert!(super::is_hashmap_type_path(&tp));

        let tp_non: TypePath = parse_quote! { Vec<u32> };
        assert!(!super::is_hashmap_type_path(&tp_non));
    }

    #[test]
    fn test_inner_boxed_type_path_build_in_types() {
        let tp: TypePath = parse_quote! { Box<u32> };
        let inner = super::inner_boxed_type_path(&tp).unwrap();
        let expected: Type = parse_quote! { u32 };
        assert_eq!(
            quote::quote!(#inner).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_inner_boxed_type_path_custom_types() {
        let tp: TypePath = parse_quote! { Box<CustomType> };
        let inner = super::inner_boxed_type_path(&tp).unwrap();
        let expected: Type = parse_quote! { CustomType };
        assert_eq!(
            quote::quote!(#inner).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_inner_vec_type_path_build_in_types() {
        let tp: TypePath = parse_quote! { Vec<u32> };
        let inner = super::inner_vec_type_path(&tp).unwrap();
        let expected: Type = parse_quote! { u32 };
        assert_eq!(
            quote::quote!(#inner).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_inner_vec_type_path_custom_types() {
        let tp: TypePath = parse_quote! { Vec<CustomType> };
        let inner = super::inner_vec_type_path(&tp).unwrap();
        let expected: Type = parse_quote! { CustomType };
        assert_eq!(
            quote::quote!(#inner).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn test_inner_hashmap_type_path_build_in_types() {
        let tp: TypePath = parse_quote! { HashMap<String, u32> };
        let (key, value) = super::inner_hashmap_type_path(&tp).unwrap();
        let expected_key: Type = parse_quote! { String };
        let expected_value: Type = parse_quote! { u32 };
        assert_eq!(
            quote::quote!(#key).to_string(),
            quote::quote!(#expected_key).to_string()
        );
        assert_eq!(
            quote::quote!(#value).to_string(),
            quote::quote!(#expected_value).to_string()
        );
    }

    #[test]
    fn test_inner_hashmap_type_path_custom_types() {
        let tp: TypePath = parse_quote! { HashMap<CustomKey, CustomValue> };
        let (key, value) = super::inner_hashmap_type_path(&tp).unwrap();
        let expected_key: Type = parse_quote! { CustomKey };
        let expected_value: Type = parse_quote! { CustomValue };
        assert_eq!(
            quote::quote!(#key).to_string(),
            quote::quote!(#expected_key).to_string()
        );
        assert_eq!(
            quote::quote!(#value).to_string(),
            quote::quote!(#expected_value).to_string()
        );
    }

    #[test]
    fn test_to_spec_type() {
        let original: TypePath = parse_quote! { MyType };
        let spec_type = super::to_spec_type(&original);
        let expected: Type = parse_quote! { MyTypeSpec };
        assert_eq!(
            quote::quote!(#spec_type).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

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
        let attrs: Vec<Attribute> = vec![parse_quote!(#[spec_mandatory])];
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
            parse_quote!(#[spec_mandatory]),
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
    fn test_has_default_attr_with_default() {
        let attrs: Vec<Attribute> = vec![parse_quote!(#[spec_default])];
        assert!(super::has_default_attr(&attrs));
    }

    #[test]
    fn test_has_default_attr_with_default_value() {
        let attrs: Vec<Attribute> = vec![parse_quote!(#[spec_default(false)])];
        assert!(super::has_default_attr(&attrs));
    }

    #[test]
    fn test_has_default_attr_without_default() {
        let attrs: Vec<Attribute> = vec![parse_quote!(#[serde(rename = "foo")])];
        assert!(!super::has_default_attr(&attrs));
    }

    #[test]
    fn test_has_default_attr_with_multiple_attrs_including_default() {
        let attrs: Vec<Attribute> = vec![
            parse_quote!(#[serde(rename = "foo")]),
            parse_quote!(#[spec_default]),
            parse_quote!(#[doc = "Some doc"]),
        ];
        assert!(super::has_default_attr(&attrs));
    }

    #[test]
    fn test_has_default_attr_with_empty_attrs() {
        let attrs: Vec<syn::Attribute> = vec![];
        assert!(!super::has_default_attr(&attrs));
    }

    #[test]
    fn test_get_default_attr_value_with_value() {
        let attrs: Vec<Attribute> = vec![parse_quote!(#[spec_default(42)])];
        let default_value = super::get_default_attr_value(&attrs).unwrap();
        let expected: TokenStream = quote! { 42 };
        assert_eq!(default_value.to_string(), expected.to_string());
    }

    #[test]
    fn test_get_default_attr_value_with_value_boolean() {
        let attrs: Vec<Attribute> = vec![parse_quote!(#[spec_default(true)])];
        let default_value = super::get_default_attr_value(&attrs).unwrap();
        let expected: TokenStream = quote! { true };
        assert_eq!(default_value.to_string(), expected.to_string());
    }

    #[test]
    fn test_get_default_attr_value_without_value() {
        let attrs: Vec<Attribute> = vec![parse_quote!(#[spec_default])];
        let default_value = super::get_default_attr_value(&attrs);
        assert!(default_value.is_none());
    }

    #[test]
    fn test_get_option_handling_mandatory() {
        let attrs: Vec<Attribute> = vec![parse_quote!(#[spec_mandatory])];
        let field_name: Ident = parse_quote! { my_field };
        let handling = super::get_option_handling(&attrs, &field_name).unwrap();
        let expected: TokenStream = quote! {
            .ok_or("Missing field 'my_field'")?
        };
        assert_eq!(handling.to_string(), expected.to_string());
    }

    #[test]
    fn test_get_option_handling_default() {
        let attrs: Vec<Attribute> = vec![parse_quote!(#[spec_default])];
        let field_name: Ident = parse_quote! { my_field };
        let handling = super::get_option_handling(&attrs, &field_name).unwrap();
        let expected: TokenStream = quote! {
            .unwrap_or_default()
        };
        assert_eq!(handling.to_string(), expected.to_string());
    }

    #[test]
    fn test_get_option_handling_default_with_value() {
        let attrs: Vec<Attribute> = vec![parse_quote!(#[spec_default(42)])];
        let field_name: Ident = parse_quote! { my_field };
        let handling = super::get_option_handling(&attrs, &field_name).unwrap();
        let expected: TokenStream = quote! {
            .unwrap_or(42)
        };
        assert_eq!(handling.to_string(), expected.to_string());
    }

    #[test]
    fn test_get_option_handling_none() {
        let attrs: Vec<Attribute> = vec![parse_quote!(#[serde(rename = "foo")])];
        let field_name: Ident = parse_quote! { my_field };
        let handling = super::get_option_handling(&attrs, &field_name);
        assert!(handling.is_none());
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
    fn test_extract_inner_with_two_generics_shall_fail() {
        let tp: syn::TypePath = parse_quote! { HashMap<u32, String> };
        let result = std::panic::catch_unwind(|| {
            super::extract_inner(&tp);
        });
        assert!(result.is_err());
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

    #[test]
    fn test_to_spec_ident() {
        let original: Ident = parse_quote! { MyType };
        let spec_ident = super::to_spec_ident(&original);
        let expected: Ident = parse_quote! { MyTypeSpec };
        assert_eq!(spec_ident.to_string(), expected.to_string());
    }
}
