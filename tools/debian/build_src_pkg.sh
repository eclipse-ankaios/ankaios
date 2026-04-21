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

# Builds an unsigned Debian source package.
# Output artifacts are placed in dist/.
#
# Required env vars:
#   ANKAIOS_VERSION   upstream version, e.g. 1.0.0
#
# Optional env vars:
#   REVISION          build revision, e.g. 1; if set, version becomes <version>-<revision>

set -e

if [ -z "${ANKAIOS_VERSION}" ]; then
    echo "Error: ANKAIOS_VERSION environment variable is not set."
    exit 1
fi

if [ -n "${REVISION}" ]; then
    DEB_VERSION="${ANKAIOS_VERSION}-${REVISION}"
else
    DEB_VERSION="${ANKAIOS_VERSION}"
fi

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
BASE_DIR="$SCRIPT_DIR/../.."
MAINTAINER="Eclipse Ankaios <ankaios-dev@eclipse.org>"
LOG_LEVEL="info"
RELEASE_URL="https://github.com/eclipse-ankaios/ankaios/releases/tag/v$ANKAIOS_VERSION"

write_control() {
    cat > "$BASE_DIR/debian/control" <<'EOF'
Source: ankaios
Section: embedded
Priority: optional
Maintainer: Eclipse Ankaios <ankaios-dev@eclipse.org>
Build-Depends: debhelper-compat (= 13),
 rustc-1.89,
 cargo-1.89,
 protobuf-compiler,
 help2man
Standards-Version: 4.6.2
Homepage: https://github.com/eclipse-ankaios/ankaios

Package: ank-server
Architecture: amd64 arm64
Depends: ${shlibs:Depends}, ${misc:Depends}
Description: Server application of Eclipse Ankaios
 Eclipse Ankaios provides workload and container orchestration for automotive
 High Performance Computing Platforms (HPCs). It offers a slim yet powerful
 solution to manage containerized applications across multiple nodes and
 virtual machines via a single API.
 .
 This package contains the Ankaios server, which acts as the central control
 point for managing workloads running on connected Ankaios agents.

Package: ank-agent
Architecture: amd64 arm64
Depends: ${shlibs:Depends}, ${misc:Depends}
Description: Agent application of Eclipse Ankaios
 Eclipse Ankaios provides workload and container orchestration for automotive
 High Performance Computing Platforms (HPCs). It offers a slim yet powerful
 solution to manage containerized applications across multiple nodes and
 virtual machines via a single API.
 .
 This package contains the Ankaios agent, which acts as an intermediate
 between the server and the workloads, being responsible with managing
 the workloads that are registered under it.

Package: ank
Architecture: amd64 arm64
Depends: ${shlibs:Depends}, ${misc:Depends}
Description: CLI application of Eclipse Ankaios
 Eclipse Ankaios provides workload and container orchestration for automotive
 High Performance Computing Platforms (HPCs). It offers a slim yet powerful
 solution to manage containerized applications across multiple nodes and
 virtual machines via a single API.
 .
 This package contains the Ankaios CLI which represents a way for users to
 access directly the state of the Ankaios cluster through the GRPC interface.

Package: ankaios
Architecture: all
Depends: ank-server (= ${binary:Version}), ank-agent (= ${binary:Version}), ank (= ${binary:Version})
Description: Eclipse Ankaios - full installation
 Meta-package that installs all Ankaios components: server, agent, and CLI.
EOF
}

write_service() {
    local description="$1" bin_dir="$2" exec_name="$3" log_level="$4"
    cat > "$BASE_DIR/debian/$exec_name.service" << EOF
[Unit]
Description=${description}
After=network.target
Wants=network.target

[Service]
Environment="RUST_LOG=${log_level}"
ExecStart=${bin_dir}/${exec_name}

[Install]
WantedBy=default.target
EOF
}

write_copyright() {
    cat > "$BASE_DIR/debian/copyright" <<EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: ankaios
Upstream-Contact: ${MAINTAINER}
Source: https://github.com/eclipse-ankaios/ankaios

Files: *
Copyright: 2026 Elektrobit Automotive GmbH
License: Apache-2.0
 On Debian systems, the full text of the Apache License 2.0 can be found
 in /usr/share/common-licenses/Apache-2.0.
EOF
}

write_rules() {
    cat > "$BASE_DIR/debian/rules" <<'EOF'
#!/usr/bin/make -f

export RUSTC = /usr/bin/rustc-1.89
export CARGO = /usr/bin/cargo-1.89
export CARGO_HOME = $(CURDIR)/debian/.cargo

RUST_HOST := $(shell $(RUSTC) --print host-tuple)

%:
	dh $@

override_dh_auto_build:
	$(CARGO) build --release --offline --target $(RUST_HOST)

override_dh_auto_test:
	# Skip tests during package build

override_dh_auto_install:
	# Server
	install -D -m 755 target/$(RUST_HOST)/release/ank-server debian/ank-server/usr/bin/ank-server
	install -D -m 644 server/config/ank-server.conf debian/ank-server/etc/ankaios/ank-server.conf
	install -D -m 644 server/config/state.yaml debian/ank-server/etc/ankaios/state.yaml
	install -D -m 644 README.md debian/ank-server/usr/share/doc/ank-server/README.md
	# Agent
	install -D -m 755 target/$(RUST_HOST)/release/ank-agent debian/ank-agent/usr/bin/ank-agent
	install -D -m 644 agent/config/ank-agent.conf debian/ank-agent/etc/ankaios/ank-agent.conf
	install -D -m 644 README.md debian/ank-agent/usr/share/doc/ank-agent/README.md
	# CLI
	install -D -m 755 target/$(RUST_HOST)/release/ank debian/ank/usr/bin/ank
	install -D -m 644 ank/config/ank.conf debian/ank/etc/ankaios/ank.conf
	install -D -m 644 README.md debian/ank/usr/share/doc/ank/README.md
	# Man pages
	tools/generate_man_pages.sh target/$(RUST_HOST)/release debian/.man
	mkdir -p debian/ank-server/usr/share/man/man8
	install -m 644 debian/.man/man8/ank-server.8 debian/ank-server/usr/share/man/man8/
	mkdir -p debian/ank-agent/usr/share/man/man8
	install -m 644 debian/.man/man8/ank-agent.8 debian/ank-agent/usr/share/man/man8/
	mkdir -p debian/ank/usr/share/man/man1
	find debian/.man/man1 -name '*.1' -exec install -m 644 {} debian/ank/usr/share/man/man1/ \;
	# Meta-Package
	install -D -m 644 README.md debian/ankaios/usr/share/doc/ankaios/README.md

override_dh_auto_clean:
	dh_auto_clean
	rm -rf debian/.man

override_dh_installsystemd:
	dh_installsystemd -p ank-server ank-server.service
	dh_installsystemd -p ank-agent ank-agent.service
EOF
    chmod +x "$BASE_DIR/debian/rules"
}

write_changelog() {
    cat > "$BASE_DIR/debian/changelog" <<EOF
ankaios (${DEB_VERSION}) unstable; urgency=low

  * New upstream release. Full changelog:
    ${RELEASE_URL}

 -- ${MAINTAINER}  $(date -R)
EOF
}

write_format_and_options() {
    mkdir -p "$BASE_DIR/debian/source"
    echo "3.0 (native)" > "$BASE_DIR/debian/source/format"
    cat > "$BASE_DIR/debian/source/options" <<'EOF'
tar-ignore = ankaios/target
tar-ignore = ankaios/.cache
tar-ignore = ankaios/dist
tar-ignore = ankaios/.git
tar-ignore = ankaios/.github
tar-ignore = ankaios/.vscode
tar-ignore = ankaios/.devcontainer
EOF
}

strip_vendor_orig() {
    # cargo vendor patches some crates' Cargo.toml and saves the original as
    # Cargo.toml.orig, recording its checksum in .cargo-checksum.json.
    # dpkg-source excludes *.orig files from the source tarball, so the binary
    # build would fail when cargo tries to verify a checksum for a file that
    # was never extracted. Strip both the files and their checksum entries.
    local _cleanup
    _cleanup=$(mktemp /tmp/strip_orig_XXXXXX.py)
    cat > "$_cleanup" << 'PYEOF'
import json, os, sys
for root, dirs, files in os.walk(sys.argv[1]):
    if ".cargo-checksum.json" not in files:
        continue
    checksum_path = os.path.join(root, ".cargo-checksum.json")
    with open(checksum_path) as f:
        data = json.load(f)
    orig_keys = [k for k in data.get("files", {}) if k.endswith(".orig")]
    if not orig_keys:
        continue
    for key in orig_keys:
        full_path = os.path.join(root, key)
        if os.path.exists(full_path):
            os.remove(full_path)
        del data["files"][key]
    with open(checksum_path, "w") as f:
        json.dump(data, f, separators=(",", ":"))
    print(f"Stripped .orig entries from {checksum_path}: {orig_keys}")
PYEOF
    python3 "$_cleanup" "$BASE_DIR/vendor"
    rm "$_cleanup"
}

# Clean up stale debian folder and recreate it
rm -rf "$BASE_DIR/debian"
mkdir -p "$BASE_DIR/debian"

write_control
write_format_and_options
write_copyright
write_service "Ankaios server" "/usr/bin" "ank-server" "${LOG_LEVEL}"
write_service "Ankaios agent"  "/usr/bin" "ank-agent"  "${LOG_LEVEL}"
write_rules
write_changelog

strip_vendor_orig

echo "Building source package (${DEB_VERSION})..."
rm -rf "$BASE_DIR/dist"
mkdir -p "$BASE_DIR/dist/src"
(cd "$BASE_DIR" && dpkg-buildpackage -S -us -uc -d)
mv "$BASE_DIR"/../ankaios_* "$BASE_DIR/dist/src/"
