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

#[cfg(test)]
mod tests {
    use internal_derive_macros::Internal;
    use internal_derive_macros::add_field;

    #[test]
    fn it_works() {
        /// Error type emitted when a mandatory field is missing.
        #[derive(Debug)]
        pub struct MissingFieldError {
            pub field: &'static str,
        }

        #[derive(Debug, Clone, Internal)]
        #[internal_derive(Debug)]
        struct Address {
            #[mandatory]
            street: Option<String>,
            additional: Option<String>,
            city: String,
            zip: String,
        }

        #[add_field(name = "extra_item", ty = "Option<String>", attrs = "#[mandatory]")]
        #[derive(Internal, Debug)]
        #[internal_derive(Debug)]
        struct Person {
            #[mandatory]
            name: Option<Vec<String>>,
            middle_name: Option<String>,
            #[mandatory]
            address: Option<Address>,
            second_address: Option<Address>,
        }

        #[derive(Internal)]
        #[internal_derive(Debug)]
        enum MyEnum {
            A(String),
            // B{#[mandatory] bla: Option<Person>, val: i32},
            C(#[mandatory] Option<Box<Person>>, Option<i32>, Vec<i32>),
            D,
        }

        let address = Address {
            street: Some("123 Main St".to_string()),
            additional: None,
            city: "Metropolis".to_string(),
            zip: "12345".to_string(),
        };

        let address_spec: AddressInternal = address.clone().try_into().unwrap();

        println!("Address: {address_spec:?}");

        let person = Person {
            name: vec!["Alice".to_string()].into(),
            middle_name: None,
            extra_item: "Something extra".to_string().into(),
            address: Some(address),
            second_address: None,
        };

        let person_spec: PersonInternal = person.try_into().unwrap();

        println!("Person Spec: {person_spec:?}");
    }
}
