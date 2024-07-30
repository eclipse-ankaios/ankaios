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
tools_dir="$base_dir/tools"
tests_dir="$base_dir/tests"
target_dir="$base_dir/target/robot_tests_result"
default_executable_dir="$base_dir/target/x86_64-unknown-linux-musl/debug"

check_executable() {
    if [[ -x "$1" ]]
    then
        echo Found $($1 --version)
    else
        echo "'$1' is not executable or found"
        exit 1
    fi
}

if [[ -z "$ANK_BIN_DIR" ]]; then
    ANK_BIN_DIR="$default_executable_dir"
    echo Use default executable directory: $ANK_BIN_DIR
fi

ANK=$ANK_BIN_DIR/ank
ANK_SERVER=$ANK_BIN_DIR/ank-server
ANK_AGENT=$ANK_BIN_DIR/ank-agent

check_executable $ANK
check_executable $ANK_SERVER
check_executable $ANK_AGENT

echo Remove old certificates and keys for stests...
rm -rf /tmp/.certs
echo done.
echo Generate certificates and keys for stests...
$tools_dir/certs/create_certs.sh /tmp/.certs
echo done.

ANK_BIN_DIR=$ANK_BIN_DIR robot --pythonpath tests --loglevel=TRACE:TRACE -d ${target_dir} "$@"
