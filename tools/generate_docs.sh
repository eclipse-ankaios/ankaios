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

# Display help message
show_help() {
    cat << EOF
Usage: $(basename "$0") [COMMAND]

Generate documentation from Protocol Buffer files and manage MkDocs documentation.

Commands:
    serve                    Start local MkDocs development server
    build                    Build static HTML documentation
    deploy                   Deploy documentation to main branch
    deploy-release <version> Deploy specific version as latest
    --help, -h               Show this help message

EOF
}

if [[ "$1" = "--help" || "$1" = "-h" ]]; then
    show_help
    exit 0
fi

if [ -z "$1" ]; then
    echo "No command provided. Exiting.."
    show_help
    exit 1
fi

script_dir=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
base_dir="$script_dir/.."
target_dir="$base_dir/build/doc"
mkdir -p "$base_dir/build/"
rm -rf "$target_dir"
echo "Generate Markdown from ./ankaios_api/proto/* ..."
cp "$base_dir/doc/" "$target_dir" -rul
protoc --plugin=protoc-gen-doc="/usr/local/bin/protoc-gen-doc" --doc_out="$target_dir/docs/reference" --doc_opt=markdown,_ankaios.proto.md --proto_path="$base_dir/ankaios_api/proto" control_api.proto ank_base.proto
echo "Generate Markdown from ./ankaios_api/proto done."

if [[ "$1" = serve ]]; then
    mkdocs serve --config-file "$target_dir/mkdocs.yml"
elif [[ "$1" = build ]]; then
    mkdocs build --config-file "$target_dir/mkdocs.yml" -d html
elif [[ "$1" = deploy ]]; then
    mike deploy --push --config-file "$target_dir/mkdocs.yml" main
elif [[ "$1" = deploy-release && ! (-z "$2") ]]; then
    echo "Deploying documentation version $2"
    mike deploy --update-aliases --push --config-file "$target_dir/mkdocs.yml" "$2" latest
fi
