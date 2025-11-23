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
    use spec_macros::Spec;

    #[test]
    fn itest_spec_derive_basic_functionality() {
        #[derive(Debug, Clone, Spec)]
        #[spec_derive(Debug, Clone, Eq, PartialEq)]
        struct Address {
            #[spec_mandatory]
            street: Option<String>,
            additional: Option<String>,
            city: String,
            zip: String,
            #[spec_default]
            notes: Option<String>,
        }

        #[derive(Spec, Debug, Clone)]
        #[spec_derive(Debug, Clone, Eq, PartialEq)]
        struct Person {
            #[spec_mandatory]
            name: Option<Vec<String>>,
            middle_name: Option<String>,
            #[spec_mandatory]
            address: Option<Address>,
            second_address: Option<Address>,
            #[spec_default(false)]
            has_children: Option<bool>,
            #[spec_default(30)]
            preferred_contact_time: Option<u32>,
        }

        let address = Address {
            street: Some("123 Main St".to_string()),
            additional: None,
            city: "Metropolis".to_string(),
            zip: "12345".to_string(),
            notes: None,
        };

        let address_spec: AddressSpec = address.clone().try_into().unwrap();
        let address_spec_expected = AddressSpec {
            street: "123 Main St".to_string(),
            additional: None,
            city: "Metropolis".to_string(),
            zip: "12345".to_string(),
            notes: String::new(),
        };
        assert_eq!(address_spec, address_spec_expected);

        let person = Person {
            name: vec!["Alice".to_string()].into(),
            middle_name: None,
            address: Some(address),
            second_address: None,
            has_children: None,
            preferred_contact_time: None,
        };
        let person_spec: PersonSpec = person.clone().try_into().unwrap();
        let person_spec_expected = PersonSpec {
            name: vec!["Alice".to_string()],
            middle_name: None,
            address: address_spec,
            second_address: None,
            has_children: false,
            preferred_contact_time: 30,
        };
        assert_eq!(person_spec, person_spec_expected);

        #[allow(clippy::large_enum_variant)]
        #[derive(Spec)]
        #[spec_derive(Debug, Eq, PartialEq)]
        enum MyEnum {
            #[spec_enum_named]
            A(String),
            #[spec_enum_named]
            B(Person),
            C(Box<Person>),
        }

        let my_enum_a = MyEnum::A("Test String".to_string());
        let my_enum_spec_a: MyEnumSpec = my_enum_a.try_into().unwrap();
        let my_enum_spec_expected_a = MyEnumSpec::A {
            a: "Test String".to_string(),
        };
        assert_eq!(my_enum_spec_a, my_enum_spec_expected_a);

        let my_enum_b = MyEnum::B(person.clone());
        let my_enum_spec_b: MyEnumSpec = my_enum_b.try_into().unwrap();
        let my_enum_spec_expected_b = MyEnumSpec::B {
            b: person_spec.clone(),
        };
        assert_eq!(my_enum_spec_b, my_enum_spec_expected_b);

        let my_enum_c = MyEnum::C(Box::new(person.clone()));
        let my_enum_spec_c: MyEnumSpec = my_enum_c.try_into().unwrap();
        let my_enum_spec_expected_c = MyEnumSpec::C(Box::new(person_spec.clone()));
        assert_eq!(my_enum_spec_c, my_enum_spec_expected_c);
    }

    #[test]
    fn itest_options_no_mandatory_and_mandatory_in_spec() {
        const CPU_USAGE: u32 = 42;

        #[derive(Spec)]
        pub struct AgentAttributes {
            pub cpu_usage: Option<CpuUsage>,
        }

        #[derive(Spec)]
        #[spec_derive(Debug)]
        pub struct CpuUsage {
            #[spec_mandatory]
            pub cpu_usage: Option<u32>,
        }

        let external = AgentAttributes {
            cpu_usage: Some(CpuUsage {
                cpu_usage: Some(CPU_USAGE),
            }),
        };

        let spec: AgentAttributesSpec = external.try_into().unwrap();

        assert_eq!(spec.cpu_usage.unwrap().cpu_usage, CPU_USAGE);
    }

    #[test]
    fn itest_spec_vector_with_custom_type() {
        #[derive(Spec)]
        #[spec_derive(Debug)]
        struct CustomType {
            #[spec_mandatory]
            value: Option<String>,
        }

        #[derive(Spec)]
        #[spec_derive(Debug)]
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

        let spec: ContainerSpec = external.try_into().unwrap();

        assert_eq!(spec.items[0].value, "Item 1".to_string());
        assert_eq!(spec.items[1].value, "Item 2".to_string());
    }

    #[test]
    fn itest_spec_hashmap_with_custom_type() {
        use std::collections::HashMap;

        #[derive(Spec)]
        #[spec_derive(Debug)]
        struct CustomType {
            #[spec_mandatory]
            value: Option<String>,
        }

        #[derive(Spec)]
        #[spec_derive(Debug)]
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

        let spec: ContainerSpec = external.try_into().unwrap();

        assert_eq!(spec.items.get("key1").unwrap().value, "Value 1".to_string());
        assert_eq!(spec.items.get("key2").unwrap().value, "Value 2".to_string());
    }

    #[test]
    fn itest_default_with_complex_expression() {
        #[derive(Spec, Debug, PartialEq)]
        #[spec_derive(Debug, PartialEq)]
        struct MyType {
            field: i32,
        }

        #[derive(Spec, Debug, PartialEq)]
        #[spec_derive(Debug, PartialEq)]
        struct MyStruct {
            #[spec_default(MyType { field: 42 })]
            my_field: Option<MyType>,
        }

        let external = MyStruct { my_field: None };

        let spec: MyStructSpec = external.try_into().unwrap();

        let expected_spec = MyStructSpec {
            my_field: MyTypeSpec { field: 42 },
        };
        assert_eq!(spec, expected_spec);

        let back_converted: MyStruct = spec.into();
        let expected_back_converted = MyStruct {
            my_field: Some(MyType { field: 42 }),
        };
        assert_eq!(back_converted, expected_back_converted);
    }
}
