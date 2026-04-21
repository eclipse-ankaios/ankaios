#!/bin/bash

# Copyright (c) 2026 Elektrobit Automotive GmbH
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

# Builds binary .deb packages from the source package produced by build_src_pkg.sh.
# Output .deb files are placed in dist/.
#
# Required env vars:
#   ANKAIOS_VERSION   upstream version, e.g. 1.0.0
#
# Optional env vars:
#   REVISION          build revision (must match the value used in build_src_pkg.sh)

set -e

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
BASE_DIR="$SCRIPT_DIR/../.."
ANKAIOS_VERSION="${ANKAIOS_VERSION:-1.0.0}"

if [ -n "${REVISION}" ]; then
    DEB_VERSION="${ANKAIOS_VERSION}-${REVISION}"
else
    DEB_VERSION="${ANKAIOS_VERSION}"
fi

dsc_file="$BASE_DIR/dist/src/ankaios_${DEB_VERSION}.dsc"

build_dir=$(mktemp -d /tmp/ankaios-build-XXXXXX)
trap 'rm -rf "$build_dir"' EXIT

echo "Building binary packages (${DEB_VERSION})..."
dpkg-source -x "$dsc_file" "$build_dir/src"
(cd "$build_dir/src" && dpkg-buildpackage -b -us -uc -d)
mv "$build_dir"/*.deb "$BASE_DIR/dist/"
