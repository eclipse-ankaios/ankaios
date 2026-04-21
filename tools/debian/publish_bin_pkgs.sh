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

# Publishes binary .deb packages from dist/ to the Nexus apt repository.
# The ankaios_*_all.deb meta-package is only uploaded when ARCH=amd64 to
# avoid duplicate uploads from other runners.
#
# Required env vars:
#   REPO_TOKEN_USERNAME   Nexus username
#   REPO_TOKEN_PASSWORD   Nexus password
#
# Optional env vars:
#   ARCH                  current build architecture (amd64 or arm64)
#   REPOSITORY            Nexus repository name (default: ankaios-apt)

set -e

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
BASE_DIR="$SCRIPT_DIR/../.."
NEXUS_URL="https://repo.eclipse.org"
REPOSITORY="${REPOSITORY:-ankaios-apt}"
ARCH="${ARCH:-}"

for deb in "$BASE_DIR"/dist/*.deb; do
    [ -f "$deb" ] || continue

    # The meta-package (ankaios_*_all.deb) is architecture-independent and
    # identical on both runners — only upload it once from the amd64 runner.
    if [[ "$deb" == *_all.deb ]] && [ "$ARCH" != "amd64" ]; then
        echo "Skipping $(basename "$deb") — published by the amd64 runner."
        continue
    fi

    echo "Uploading $(basename "$deb")..."
    http_code=$(curl -u "$REPO_TOKEN_USERNAME:$REPO_TOKEN_PASSWORD" \
        -w '%{http_code}' \
        -H "Content-Type: multipart/form-data" \
        --data-binary "@$deb" \
        "$NEXUS_URL/repository/$REPOSITORY/" \
        -o /dev/null)

    if [ "$http_code" -lt 200 ] || [ "$http_code" -ge 300 ]; then
        echo "Upload failed for $(basename "$deb") (HTTP $http_code)"
        exit 1
    fi
    echo "  -> HTTP $http_code"
done

echo "Done."
