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

# It is also recommended to activate traces with ```RUST_LOG=debug``` before you generate the report.
# This way the trace report also includes trace lines.
# Without activated traces, the report complains that the trace lines are not covered by any test.
RUST_LOG=debug cargo llvm-cov nextest --ignore-filename-regex "$(cat << 'EOF' | grep -v -P '^#|^[[:space:]]*$|^$' | paste -sd '|'
/main.rs

# Test utilities not part of production code
/test_utils.rs

# Command line interface definition defined with third-party library
/cli.rs

# Primitive operations already tested in higher level components
/objects/agent_map.rs

EOF
)" "$@"
