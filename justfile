# Copyright (c) 2024 Elektrobit Automotive GmbH
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

vendor_dir := "vendor"
config := ".cargo/config.toml"
oft_dirs := "-type d \\( -name 'src' -o -name 'doc' -o -name 'tests' \\) -not -path './doc'"
oft_args := "-a swdd,impl,utest,itest,stest"

all: check-test-images check-licenses check-advisories check-copyright-headers clippy test stest build-release

# Perform debug build
build:
    cargo build

# Perform release build
build-release:
    cargo build --release

# Cleans the project and system using cargo clean and the ankaios-clean dev command
clean:
    cargo clean
    ./tools/dev_scripts/ankaios-clean
    rm -rf build
    rm -rf {{vendor_dir}}
    # Revert changes for vendored sources
    git checkout -- {{config}}
    rm -rf dist

# Check licenses of dependencies
check-licenses:
    cargo deny check licenses

# Check advisories as part of https://rustsec.org/advisories/
check-advisories:
    cargo deny check advisories

# Prevent non ghcr.io images to be used in test due to rate limit problem
check-test-images:
    test -z "$(find tests/resources/configs -type f -exec grep -H -P 'image: (?!ghcr\.io/|image_typo:latest)' {} \;)"

# Check for the presence of a copyright header
check-copyright-headers:
    ./tools/check_copyright_headers.sh

# Run all tests
test: utest stest

# Run unit tests
utest:
    RUST_LOG=debug cargo nextest --config-file nextest.toml run

# Build debug and run all system tests
stest filter="*" tests="tests": build build-stest-image
    just stest-only "{{ filter }}" "{{ tests }}"

# Builds the tester image used within the system tests
build-stest-image:
    #!/usr/bin/env bash
    SRC_HASH="$(./tools/control_interface_workload_hash.sh)"
    podman pull "ghcr.io/eclipse-ankaios/control_interface_tester:$SRC_HASH"
    if [ $? -ne 0 ]; then
        podman build -t "ghcr.io/eclipse-ankaios/control_interface_tester:$SRC_HASH" --build-arg=SRC_HASH="$SRC_HASH" . -f tests/resources/control_interface_tester/Dockerfile
        echo 'Had to build control_interface_tester image. Consider uploading it with `podman push ghcr.io/eclipse-ankaios/control_interface_tester:'"$SRC_HASH"'`'
    fi

# Only execute the stests without building
stest-only filter="*" tests="tests":
    ./tools/run_robot_tests.sh --test "{{ filter }}" "{{ tests }}"

# Run clippy code checks
clippy:
    cargo clippy --all-targets --no-deps --all-features -- -D warnings

# Generate test coverage report
coverage:
    tools/generate_test_coverage_report.sh test --html

# Create requirement tracing report
trace-requirements report="build/req/req_tracing_report.html":
    mkdir -p $(dirname "{{ report }}")
    oft trace $(find . {{oft_dirs}}) {{oft_args}} -o html -f "{{ report }}" || true

# Compare requirement tracing report from current branch with main branch
compare-requirements:
    #!/usr/bin/env bash
    set -e
    # Get main branch
    maindir=$(mktemp -d)
    git fetch origin main --quiet
    git worktree add "$maindir" main --quiet
    # Create requirement tracing report for main
    mkdir -p build/req
    oft trace $(find "$maindir" {{oft_dirs}}) {{oft_args}} -o aspec -f build/req/main.xml || true
    # Create requirement tracing report for current branch
    oft trace $(find . {{oft_dirs}}) {{oft_args}} -o aspec -f build/req/current.xml || true
    # Compare
    python3 tools/compare_req_tracing.py build/req/main.xml  build/req/current.xml
    # Cleanup
    git worktree remove "$maindir"
    rm -rf "$maindir"

# Vendor all dependencies and create source archive
vendor:
    #!/bin/sh -e
    mkdir -p dist
    cargo vendor {{vendor_dir}}
    if ! grep vendored-sources {{config}}; then
      echo '\n[source.crates-io]\nreplace-with = "vendored-sources"\n\n[source.vendored-sources]\ndirectory = "{{vendor_dir}}"' >> {{config}};
    fi
    if [ "$GITHUB_REF_TYPE" = "tag" ]; then
        # remove the leading 'v' from the tag
        VERSION=$(expr substr "$GITHUB_REF_NAME" 2 100)
        SOURCE_ARCHIVE=dist/ankaios-vendored-source-${VERSION}.tar.gz
        SOURCE_ARCHIVE_BASE=ankaios-${VERSION}
    else
        SOURCE_ARCHIVE=dist/ankaios-vendored-source.tar.gz
        SOURCE_ARCHIVE_BASE=ankaios
    fi
    # Create a source archive with the vendored dependencies, the source code and the modified
    # .cargo/config.toml file using the folder structure ankaios[-<version>]/*
    # Note: The order is important in the next line. --exclude only affects
    #       items mentioned after it. So we can include .cargo/config.toml
    #       while excluding the rest of the folder.
    tar -czf ${SOURCE_ARCHIVE} --transform "s,^,${SOURCE_ARCHIVE_BASE}/," .cargo/config.toml {{vendor_dir}} --exclude=.cargo --exclude-vcs --exclude-vcs-ignores .

# Generate and serve documentation
serve-docs:
    ./tools/generate_docs.sh serve
