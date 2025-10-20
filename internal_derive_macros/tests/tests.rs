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
        #[derive(Debug, Clone, Internal)]
        #[internal_derive(Debug, Clone)]
        struct Address {
            #[internal_mandatory]
            street: Option<String>,
            additional: Option<String>,
            city: String,
            zip: String,
        }

        #[add_field(
            name = "extra_item",
            ty = "Option<String>",
            attrs = "#[internal_mandatory]"
        )]
        #[derive(Internal, Debug, Clone)]
        #[internal_derive(Debug, Clone)]
        struct Person {
            #[internal_mandatory]
            name: Option<Vec<String>>,
            middle_name: Option<String>,
            #[internal_mandatory]
            address: Option<Address>,
            second_address: Option<Address>,
        }

        #[allow(clippy::large_enum_variant)]
        #[derive(Internal)]
        #[internal_derive(Debug)]
        enum MyEnum {
            #[internal_enum_named]
            A(String),
            // B{#[mandatory] bla: Option<Person>, val: i32},
            B(Person),
            C(Box<Person>),
            D,
            // E(Vec<Person>),
        }

        // Remove from here

        // to here
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

        let person_internal: PersonInternal = person.clone().try_into().unwrap();

        let _my_enum_internal_b = MyEnumInternal::B(person_internal.clone());
        let _my_enum_internal_c = MyEnumInternal::C(Box::new(person_internal.clone()));

        println!("Person Spec: {person_internal:?}");
    }

    #[test]
    fn test_options_kept_in_internal() {
        const CPU_USAGE: u32 = 42;

        #[derive(Internal)]
        pub struct AgentAttributes {
            pub cpu_usage: Option<CpuUsage>,
        }

        #[derive(Internal)]
        #[internal_derive(Debug)]
        pub struct CpuUsage {
            #[internal_mandatory]
            pub cpu_usage: Option<u32>,
        }

        let external = AgentAttributes {
            cpu_usage: Some(CpuUsage {
                cpu_usage: Some(CPU_USAGE),
            }),
        };

        let internal: AgentAttributesInternal = external.try_into().unwrap();

        assert_eq!(internal.cpu_usage.unwrap().cpu_usage, CPU_USAGE);
    }

    #[test]
    fn test_internal_vector_with_custom_type() {
        #[derive(Internal)]
        #[internal_derive(Debug)]
        struct CustomType {
            #[internal_mandatory]
            value: Option<String>,
        }

        #[derive(Internal)]
        #[internal_derive(Debug)]
        struct Container {
            items: Vec<CustomType>,
        }

        let external = Container {
            items: vec![
                CustomType {
                    value: Some("Item 1".to_string()),
                },
                CustomType {
                    value: Some("Item 2".to_string()),
                },
            ],
        };

        let internal: ContainerInternal = external.try_into().unwrap();

        assert_eq!(
            internal.items[0].value,
            "Item 1".to_string()
        );
        assert_eq!(
            internal.items[1].value,
            "Item 2".to_string()
        );
    }

    #[test]
    fn test_internal_hashmap_with_custom_type() {

        use std::collections::HashMap;

        #[derive(Internal)]
        #[internal_derive(Debug)]
        struct CustomType {
            #[internal_mandatory]
            value: Option<String>,
        }

        #[derive(Internal)]
        #[internal_derive(Debug)]
        struct Container {
            items: HashMap<String, CustomType>,
        }

        let mut external_items = HashMap::new();
        external_items.insert(
            "key1".to_string(),
            CustomType {
                value: Some("Value 1".to_string()),
            },
        );
        external_items.insert(
            "key2".to_string(),
            CustomType {
                value: Some("Value 2".to_string()),
            },
        );

        let external = Container {
            items: external_items,
        };

        let internal: ContainerInternal = external.try_into().unwrap();

        assert_eq!(
            internal.items.get("key1").unwrap().value,
            "Value 1".to_string()
        );
        assert_eq!(
            internal.items.get("key2").unwrap().value,
            "Value 2".to_string()
        );
    }
}
