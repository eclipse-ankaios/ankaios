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

FROM ghcr.io/eclipse-ankaios/devcontainer-base:0.5.0 as builder
WORKDIR /usr/src/myapp
COPY examples/workload_api_example examples/workload_api_example
COPY api api
RUN cd examples/workload_api_example && cargo build --release

FROM debian:bullseye
COPY --from=builder  /usr/src/myapp/examples/workload_api_example/target/release/ankaios-workload-api-example /usr/local/bin
ENTRYPOINT /usr/local/bin/ankaios-workload-api-example
