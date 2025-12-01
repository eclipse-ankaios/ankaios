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

mod proto_annotations;
pub use proto_annotations::setup_proto_annotations;

mod proto_macro_fixes;
pub use proto_macro_fixes::fix_proto_enum_serialization;

mod spec_structs;
pub use spec_structs::setup_spec_objects;

mod schema_annotations;
pub use schema_annotations::setup_schema_annotations;
