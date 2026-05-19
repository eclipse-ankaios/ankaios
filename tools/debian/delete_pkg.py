#!/usr/bin/env python3

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

# Searches for and deletes Debian packages from the Nexus apt repository.
# Handles paginated search results automatically.
#
# Required env vars:
#   REPO_TOKEN_USERNAME   Nexus username
#   REPO_TOKEN_PASSWORD   Nexus password
#
# Optional env vars:
#   PACKAGE               package name filter, e.g. ank-server (default: * = all)
#   VERSION               version filter, e.g. 1.0.0 (default: * = all)
#   ARCHITECTURE          architecture filter, e.g. amd64 (default: * = all)
#   REVISION              build revision filter, e.g. 1 (default: empty = not filtered)
#   REPOSITORY            Nexus repository name (default: ankaios-apt)

import json, os, sys
import urllib.request, urllib.parse, urllib.error, base64

NEXUS_URL = "https://repo.eclipse.org"

username = os.environ["REPO_TOKEN_USERNAME"]
password = os.environ["REPO_TOKEN_PASSWORD"]
package = os.environ.get("PACKAGE", "*")
version = os.environ.get("VERSION", "*")
architecture = os.environ.get("ARCHITECTURE", "*")
revision = os.environ.get("REVISION", "")
repository = os.environ.get("REPOSITORY", "ankaios-apt")

if revision and version == "*":
    print("Error: REVISION requires VERSION to be set.", file=sys.stderr)
    sys.exit(1)

full_version = f"{version}-{revision}" if revision else version

params = {"repository": repository}
if package != "*":
    params["name"] = package
if full_version != "*":
    params["version"] = full_version

credentials = base64.b64encode(f"{username}:{password}".encode()).decode()
headers = {"Authorization": f"Basic {credentials}", "Accept": "application/json"}

continuation_token = None
total_deleted = 0
failed = 0

while True:
    query = dict(params)
    if continuation_token:
        query["continuationToken"] = continuation_token

    url = f"{NEXUS_URL}/service/rest/v1/search?" + urllib.parse.urlencode(query)
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req) as resp:
        data = json.load(resp)

    items = data.get("items", [])
    if not items and not continuation_token:
        print("No packages found matching the criteria.")
        break

    for item in items:
        component_id = item["id"]
        name = item.get("name", "?")
        ver = item.get("version", "?")
        arch = item.get("group", "?")  # Nexus uses "group" for architecture in apt repos

        if architecture != "*" and arch != architecture:
            continue

        print(f"Deleting {name} {ver} {arch} ({component_id})...")
        del_req = urllib.request.Request(
            f"{NEXUS_URL}/service/rest/v1/components/{component_id}",
            method="DELETE",
            headers=headers,
        )
        try:
            with urllib.request.urlopen(del_req) as resp:
                pass
            total_deleted += 1
        except urllib.error.HTTPError as e:
            print(f"Error: failed to delete {name} {ver} {arch} ({component_id}): HTTP {e.code} {e.reason}", file=sys.stderr)
            failed += 1

    continuation_token = data.get("continuationToken")
    if not continuation_token:
        break

print(f"Deleted {total_deleted} component(s).")
if failed:
    print(f"Failed to delete {failed} component(s).", file=sys.stderr)
    sys.exit(1)
