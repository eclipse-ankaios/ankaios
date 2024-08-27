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

# Note:
#
# This Makefile is work in progress. Currently it contains only a small subset
# of all steps.

all: check-test-images check-licenses

check-licenses:
	cargo deny check licenses

# Prevent non ghcr.io images to be used in test due to rate limit problem
check-test-images:
	test -z "$(shell find tests/resources/configs -type f -exec grep -H -P 'image: (?!ghcr\.io/|image_typo:latest)' {} \;)"

