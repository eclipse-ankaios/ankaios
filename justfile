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

all: check-test-images check-licenses clippy test stest build-release

# Perform debug build
build:
    cargo build

# Perform release build
build-release:
    cargo build --release

clean:
    cargo clean
    ./tools/dev_scripts/ankaios-clean
    rm -rf build

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

# Run unit tests
test:
    cargo nextest run

# Build debug and run all system tests
stest: build stest-only

# Only execute the stests without building
stest-only tests="tests":
    ./tools/run_robot_tests.sh {{ tests }}

# Run clippy code checks
clippy:
    cargo clippy --all-targets --no-deps --all-features -- -D warnings

# Generate test coverage report
coverage:
    tools/generate_test_coverage_report.sh test --html

# Create requirement tracing report
trace-requirements report="build/req/req_tracing_report.html":
    mkdir -p $(dirname "{{ report }}")
    oft trace $(find . -type d \( -name "src" -o -name "doc" -o -name "tests" \) -not -path './doc') -a swdd,impl,utest,itest,stest -o html -f "{{ report }}" || true
