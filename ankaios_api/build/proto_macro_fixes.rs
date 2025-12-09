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

use tonic_prost_build::Builder;

/// Fix the enum serialization for fields/variants
/// of the specified types as proto is generating them as integers.
///
/// ## Example
///
/// ```rust
/// pub struct StateRule {
///     #[prost(enumeration = "ReadWriteEnum", tag = "1")]
///     pub operation: i32, -> operation: ReadWriteEnum,
/// }
/// ```
pub fn fix_proto_enum_serialization(builder: Builder) -> Builder {
    builder
        .type_attribute("Workload", "#[spec_macros::fix_enum_serialization]")
        .type_attribute("StateRule", "#[spec_macros::fix_enum_serialization]")
        .type_attribute("Dependencies", "#[spec_macros::fix_enum_serialization]")
        .type_attribute(
            "ExecutionStateEnum",
            "#[spec_macros::fix_enum_serialization]",
        )
}
