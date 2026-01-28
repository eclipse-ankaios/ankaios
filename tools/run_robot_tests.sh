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
target_dir="$base_dir/target/robot_tests_result"
default_executable_dir="$base_dir/target/x86_64-unknown-linux-musl/debug"

if [[ -z "$ANK_BIN_DIR" ]]; then
    ANK_BIN_DIR="$default_executable_dir"
    echo Use default executable directory: $ANK_BIN_DIR
fi

# run robot tests
ANK_BIN_DIR=$ANK_BIN_DIR robot --pythonpath tests --loglevel=TRACE:TRACE -x xunitOut.xml -d ${target_dir} "$@"
