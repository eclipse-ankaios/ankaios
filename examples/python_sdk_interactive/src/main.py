# Copyright (c) 2025 Elektrobit Automotive GmbH
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

from time import sleep
import sys, signal

still_sleepy = True

def signal_handler(sig, frame):
    still_sleepy = False
    sys.exit(0)

# Add a SIGTERM handler to allow a quick shutdown
signal.signal(signal.SIGTERM, signal_handler)

while still_sleepy:
    sleep(1)
