#!/bin/bash

# Copyright (c) 2023 Elektrobit Automotive GmbH
#
# This program and the accompanying materials are made available under the
# terms of the Apache License, Version 2.0 which is available at
# https://www.apache.org/licenses/LICENSE-2.0.
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
# License for the specific language governing permissions and limitations
# under the License.
#
# SPDX-License-Identifier: Apache-2.0

set -e

script_dir=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
base_dir="$script_dir/.."

# Note that the file paths and the name are ignored for the final hash calculation
cd "$base_dir"
find "tests/resources/control_interface_tester" \
    "api/proto" \
    "api/build.rs" \
    "api/build" \
    -type f \
| grep -v 'README.md' \
| sort \
| xargs sha256sum \
| awk '{print $1}' \
| sha256sum \
| sed 's/  -//'
