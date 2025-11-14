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

use schemars::generate::{SchemaGenerator, SchemaSettings};
use std::path::PathBuf;

const SCHEMA_NAME: &str = "ank_schema.json";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure the schema generator to use the latest json schema specification
    let settings = SchemaSettings::draft2020_12();
    let generator = SchemaGenerator::new(settings);

    // Generate schema
    let schema = generator.into_root_schema_for::<api::ank_base::StateInternal>();

    // Write schema to file
    let schemars_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("schemas");
    std::fs::create_dir_all(&schemars_dir).unwrap();
    std::fs::write(
        schemars_dir.join(SCHEMA_NAME),
        serde_json::to_string_pretty(&schema)?,
    )?;

    Ok(())
}
