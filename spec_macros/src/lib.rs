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
use syn::{DeriveInput, parse_macro_input};

mod utils;

mod derive_spec;
mod fix_enum_serialization;

#[proc_macro_derive(
    Spec,
    attributes(
        spec_mandatory,
        spec_enum_named,
        spec_derive,
        spec_type_attr,
        spec_field_attr,
    )
)]
pub fn derive_spec(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive_spec::derive_spec(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_attribute]
pub fn fix_enum_serialization(attr: TokenStream, item: TokenStream) -> TokenStream {
    assert!(
        attr.is_empty(),
        "The fix_enum_serialization attribute does not take any arguments"
    );

    let input = parse_macro_input!(item as DeriveInput);

    fix_enum_serialization::fix_prost_enum_serialization(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
