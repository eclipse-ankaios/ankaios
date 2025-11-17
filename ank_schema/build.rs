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

use std::path::PathBuf;

const SCHEMA_NAME: &str = "ank_schema.json";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let generator = schemars::generate::SchemaSettings::draft07().into_generator();

    let schema = generator.into_root_schema_for::<api::ank_base::StateInternal>();

    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let target_dir = out_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .ok_or("Failed to get target dir")?;

    let schema_path = target_dir.join(SCHEMA_NAME);
    std::fs::write(schema_path, serde_json::to_string_pretty(&schema)?)?;

    Ok(())
}
