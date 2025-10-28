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
    fn itest_internal_derive_and_add_field_basic_functionality() {
        #[derive(Debug, Clone, Internal)]
        #[internal_derive(Debug, Clone, Eq, PartialEq)]
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
        #[internal_derive(Debug, Clone, Eq, PartialEq)]
        struct Person {
            #[internal_mandatory]
            name: Option<Vec<String>>,
            middle_name: Option<String>,
            #[internal_mandatory]
            address: Option<Address>,
            second_address: Option<Address>,
        }

        let address = Address {
            street: Some("123 Main St".to_string()),
            additional: None,
            city: "Metropolis".to_string(),
            zip: "12345".to_string(),
        };

        let address_internal: AddressInternal = address.clone().try_into().unwrap();
        let address_internal_expected = AddressInternal {
            street: "123 Main St".to_string(),
            additional: None,
            city: "Metropolis".to_string(),
            zip: "12345".to_string(),
        };
        assert_eq!(address_internal, address_internal_expected);

        let person = Person {
            name: vec!["Alice".to_string()].into(),
            middle_name: None,
            extra_item: Some("Something extra".to_string()),
            address: Some(address),
            second_address: None,
        };
        let person_internal: PersonInternal = person.clone().try_into().unwrap();
        let person_internal_expected = PersonInternal {
            name: vec!["Alice".to_string()],
            middle_name: None,
            extra_item: "Something extra".to_string(),
            address: address_internal,
            second_address: None,
        };
        assert_eq!(person_internal, person_internal_expected);

        #[allow(clippy::large_enum_variant)]
        #[derive(Internal)]
        #[internal_derive(Debug, Eq, PartialEq)]
        enum MyEnum {
            #[internal_enum_named]
            A(String),
            #[internal_enum_named]
            B(Person),
            C(Box<Person>),
            D,
        }

        let my_enum_a = MyEnum::A("Test String".to_string());
        let my_enum_internal_a: MyEnumInternal = my_enum_a.try_into().unwrap();
        let my_enum_internal_expected_a = MyEnumInternal::A {
            a: "Test String".to_string(),
        };
        assert_eq!(my_enum_internal_a, my_enum_internal_expected_a);

        let my_enum_b = MyEnum::B(person.clone());
        let my_enum_internal_b: MyEnumInternal = my_enum_b.try_into().unwrap();
        let my_enum_internal_expected_b = MyEnumInternal::B {
            b: person_internal.clone(),
        };
        assert_eq!(my_enum_internal_b, my_enum_internal_expected_b);

        let my_enum_c = MyEnum::C(Box::new(person.clone()));
        let my_enum_internal_c: MyEnumInternal = my_enum_c.try_into().unwrap();
        let my_enum_internal_expected_c = MyEnumInternal::C(Box::new(person_internal.clone()));
        assert_eq!(my_enum_internal_c, my_enum_internal_expected_c);
    }

    #[test]
    fn itest_options_no_mandatory_and_mandatory_in_internal() {
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
    fn itest_internal_vector_with_custom_type() {
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

        assert_eq!(internal.items[0].value, "Item 1".to_string());
        assert_eq!(internal.items[1].value, "Item 2".to_string());
    }

    #[test]
    fn itest_internal_hashmap_with_custom_type() {
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
