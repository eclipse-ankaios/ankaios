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
use quote::quote;
use syn::{
    parse_macro_input, spanned::Spanned, Attribute, Expr, ExprLit, Fields, ItemStruct, Lit,
    MetaNameValue, Token, parse::{Parse, ParseStream, Parser, Result as ParseResult}, punctuated::Punctuated
};

struct AddFieldArgs {
    name: syn::Ident,
    ty: syn::Type,
    attrs: Vec<Attribute>,
}

impl Parse for AddFieldArgs {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        let args = Punctuated::<MetaNameValue, Token![,]>::parse_terminated(input)?;

        let mut name = None;
        let mut ty = None;
        let mut attrs = Vec::new();

        for arg in args {
            let ident = arg.path.get_ident().map(|id| id.to_string());
            match (ident.as_deref(), &arg.value) {
                (Some("name"), Expr::Lit(ExprLit {
                    lit: Lit::Str(lit_str), ..
                })) => {
                    name = Some(syn::Ident::new(&lit_str.value(), lit_str.span()));
                }

                (Some("ty"), Expr::Lit(ExprLit {
                    lit: Lit::Str(lit_str), ..
                })) => {
                    ty = Some(syn::parse_str::<syn::Type>(&lit_str.value())?);
                }

                (Some("attrs"), Expr::Lit(ExprLit {
                    lit: Lit::Str(lit_str), ..
                })) => {
                    let attr_tokens: TokenStream = lit_str.value().parse().unwrap();
                    let parsed_attrs: Vec<Attribute> =
                        syn::Attribute::parse_outer.parse_str(&attr_tokens.to_string())?;
                    attrs = parsed_attrs;
                }

                _ => {
                    return Err(syn::Error::new(arg.span(), "Invalid attribute format"));
                }
            }
        }

        Ok(AddFieldArgs {
            name: name.expect("Missing `name` parameter"),
            ty: ty.expect("Missing `ty` parameter"),
            attrs,
        })
    }
}

pub fn add_field(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as AddFieldArgs);
    let mut input_struct = parse_macro_input!(item as ItemStruct);

    if let Fields::Named(ref mut fields_named) = input_struct.fields {
        let field = syn::Field {
            attrs: args.attrs,
            vis: syn::Visibility::Public(
                 <Token![pub]>::default(),
            ),
            ident: Some(args.name),
            colon_token: Some(<Token![:]>::default()),
            ty: args.ty,
            mutability: syn::FieldMutability::None,
        };

        fields_named.named.push(field);
    } else {
        return syn::Error::new_spanned(
            input_struct,
            "#[add_field] only supports structs with named fields",
        )
        .to_compile_error()
        .into();
    };

    let expanded = quote! {
        #input_struct
    };

    // TODO trace the modified token stream

    expanded.into()
}
