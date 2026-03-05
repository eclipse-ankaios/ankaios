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

# This script handles the creation of the source debian packages.
# For the binary packages, they will be generated automatically by LaunchPad.
#
# bin destination: /usr/bin
# config destination: /etc/ankaios
# service destination: /lib/systemd/system

set -e

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
BASE_DIR="$SCRIPT_DIR/../.."
MAINTAINER="Eclipse Ankaios <ankaios-dev@eclipse.org>"
ANKAIOS_VERSION="${ANKAIOS_VERSION:-1.0.0}"
RELEASE_URL="https://github.com/eclipse-ankaios/ankaios/releases/tag/v$ANKAIOS_VERSION"
LOG_LEVEL="info"

# Space-separated list of Ubuntu series to build source packages for.
UBUNTU_SERIES="${UBUNTU_SERIES:-noble jammy}"
# PPA build revision. Increment when re-uploading the same upstream version to
# the same series.
PPA_BUILD="${PPA_BUILD:-1}"

write_service() {
    local description="$1" bin_dir="$2" exec_name="$3" log_level="$4"
    cat > "$BASE_DIR/debian/$exec_name.service" << EOF
[Unit]
Description=${description}

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
Copyright: 2023 Elektrobit Automotive GmbH
License: Apache-2.0
 On Debian systems, the full text of the Apache License 2.0 can be found
 in /usr/share/common-licenses/Apache-2.0.
EOF
}

write_rules() {
    cat > "$BASE_DIR/debian/rules" <<'EOF'
#!/usr/bin/make -f

# Use Rust 1.89 versioned packages from apt.
export RUSTC = /usr/bin/rustc-1.89
export CARGO = /usr/bin/cargo-1.89
export CARGO_HOME = $(CURDIR)/debian/.cargo

# Derive the Rust target triple from DEB_HOST_GNU_TYPE.
DEB_HOST_GNU_TYPE := $(shell dpkg-architecture -qDEB_HOST_GNU_TYPE)
RUST_HOST := $(subst -linux-gnu,-unknown-linux-gnu,$(DEB_HOST_GNU_TYPE))

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
	mkdir -p debian/ank-server/usr/share/man/man8
	help2man --no-info --section=8 --name="Ankaios server" \
		target/$(RUST_HOST)/release/ank-server \
		-o debian/ank-server/usr/share/man/man8/ank-server.8
	# Agent
	install -D -m 755 target/$(RUST_HOST)/release/ank-agent debian/ank-agent/usr/bin/ank-agent
	install -D -m 644 agent/config/ank-agent.conf debian/ank-agent/etc/ankaios/ank-agent.conf
	install -D -m 644 README.md debian/ank-agent/usr/share/doc/ank-agent/README.md
	mkdir -p debian/ank-agent/usr/share/man/man8
	help2man --no-info --section=8 --name="Ankaios agent" \
		target/$(RUST_HOST)/release/ank-agent \
		-o debian/ank-agent/usr/share/man/man8/ank-agent.8
	# CLI
	install -D -m 755 target/$(RUST_HOST)/release/ank debian/ank/usr/bin/ank
	install -D -m 644 ank/config/ank.conf debian/ank/etc/ankaios/ank.conf
	install -D -m 644 README.md debian/ank/usr/share/doc/ank/README.md
	mkdir -p debian/ank/usr/share/man/man1
	help2man --no-info --section=1 --name="Ankaios CLI" \
		target/$(RUST_HOST)/release/ank \
		-o debian/ank/usr/share/man/man1/ank.1
	# Meta-Package
	install -D -m 644 README.md debian/ankaios/usr/share/doc/ankaios/README.md

override_dh_installsystemd:
	dh_installsystemd -p ank-server ank-server.service
	dh_installsystemd -p ank-agent ank-agent.service
EOF
    chmod +x "$BASE_DIR/debian/rules"
}

write_changelog() {
    local series="$1" deb_version="$2"
    cat > "$BASE_DIR/debian/changelog" <<EOF
ankaios (${deb_version}) ${series}; urgency=low

  * New upstream release. Full changelog:
    ${RELEASE_URL}

 -- ${MAINTAINER}  $(date -R)
EOF
}

write_format_and_options() {
    mkdir -p "$BASE_DIR/debian/source"
    echo "3.0 (native)" > "$BASE_DIR/debian/source/format"
    # Exclude build artifacts and caches from the source tarball.
    cat > "$BASE_DIR/debian/source/options" <<'EOF'
tar-ignore = ./target
tar-ignore = ./.cache
tar-ignore = ./dist
tar-ignore = ./.git
tar-ignore = ./.github
tar-ignore = ./.vscode
tar-ignore = ./.devcontainer
EOF
}

# Clean up stale debian folder and recreate it
rm -rf "$BASE_DIR/dist"
rm -rf "$BASE_DIR/debian"
mkdir -p "$BASE_DIR/debian"

# Populate the series-independent parts of the debian structure
cp "$SCRIPT_DIR/control_file" "$BASE_DIR/debian/control"
write_format_and_options
write_copyright
write_service "Ankaios server" "/usr/bin" "ank-server" "${LOG_LEVEL}"
write_service "Ankaios agent"  "/usr/bin" "ank-agent"  "${LOG_LEVEL}"
write_rules

# cargo vendor patches some crates' Cargo.toml and saves the original as
# Cargo.toml.orig, recording its checksum in .cargo-checksum.json.
# dpkg-source excludes *.orig files from the source tarball, so the Launchpad
# binary build would fail when cargo tries to verify a checksum for a file that
# was never extracted. Strip both the files and their checksum entries.
# Using a temp file avoids heredoc-in-subshell quoting issues.
_CLEANUP=$(mktemp /tmp/strip_orig_XXXXXX.py)
cat > "$_CLEANUP" << 'PYEOF'
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
python3 "$_CLEANUP" "$BASE_DIR/vendor"
rm "$_CLEANUP"

# Write the GPG passphrase to a temp file so debsign can sign non-interactively
# via --pinentry-mode loopback.
_PASSFILE=$(mktemp)
chmod 600 "$_PASSFILE"
echo "$GPG_PASSPHRASE" > "$_PASSFILE"
trap 'rm -f "$_PASSFILE"' EXIT

# Build one source package per Ubuntu series.
# Version format: <upstream>~<series><ppa_build>  e.g. 1.0.3~noble1
mkdir -p "$BASE_DIR/dist"
for series in $UBUNTU_SERIES; do
    deb_version="${ANKAIOS_VERSION}~${series}${PPA_BUILD}"
    echo -e "\nBuilding source package for ${series} (${deb_version})..."
    write_changelog "$series" "$deb_version"

    # Build unsigned; debsign below applies the GPG signature required for PPA upload.
    (cd "$BASE_DIR" && dpkg-buildpackage -S -us -uc -d)
    mv "$BASE_DIR"/../ankaios_* "$BASE_DIR/dist/"

    echo "Signing source package for ${series}..."
    changes_file="$BASE_DIR/dist/ankaios_${deb_version}_source.changes"
    debsign -k "$GPG_KEY_ID" -p"gpg --pinentry-mode loopback --passphrase-file $_PASSFILE" "$changes_file"
done
