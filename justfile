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

all: check-test-images check-licenses test stest build-release

build:
    cargo build

build-release:
    cargo build --release

clean:
    cargo clean
    ./tools/dev_scripts/ankaios-clean
    rm -rf build

check-licenses:
    cargo deny check licenses

# Prevent non ghcr.io images to be used in test due to rate limit problem
check-test-images:
    test -z "$(find tests/resources/configs -type f -exec grep -H -P 'image: (?!ghcr\.io/|image_typo:latest)' {} \;)"

check-copyright-headers:
	./tools/check_copyright_headers.sh

test:
    cargo nextest run

# Build debug and run all system tests
stest: build stest-only

# only execute the stests without building
stest-only:
    ./tools/run_robot_tests.sh tests
