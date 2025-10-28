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

mod internal_enum;
mod internal_struct;
mod utils;

use internal_enum::derive_internal_enum;
use internal_struct::derive_internal_struct;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DataEnum, DataStruct, DeriveInput, Fields};

use crate::derive_internal::utils::{get_internal_type_attrs, has_skip_try_from};

pub fn derive_internal(input: DeriveInput) -> syn::Result<TokenStream> {
    let orig_name = input.ident;
    let vis = input.vis.clone();
    let skip_try_from = has_skip_try_from(&input.attrs);
    let new_type_attrs = get_internal_type_attrs(&input.attrs);

    let internal = match input.data {
        Data::Struct(DataStruct {
            fields: Fields::Named(fields),
            ..
        }) => derive_internal_struct(fields, orig_name, vis, new_type_attrs, skip_try_from)?,
        Data::Enum(DataEnum { variants, .. }) => {
            derive_internal_enum(variants, orig_name, vis, new_type_attrs, skip_try_from)?
        }
        _ => Err(syn::Error::new_spanned(
            orig_name,
            "Internal derive only supports named structs and enums",
        ))?,
    };

    let expanded = quote! {
        #internal
    };

    Ok(expanded)
}
