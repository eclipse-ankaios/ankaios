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

"""
Compare two requirement tracing reports in specobject format
(OpenFastTrace "-o aspec") and print the requirements in the
second report, that miss coverage which is different to first
report.

Example usage:
    python3 compare_req_tracing.py old-report.xml new-report.xml
"""

import sys
import xml.etree.ElementTree as ET

def extract_specobjects(xml_path):
    tree = ET.parse(xml_path)
    root = tree.getroot()
    result = {}
    for specobjects in root.findall(".//specobjects"):
        doctype = specobjects.attrib.get("doctype", "")
        for specobject in specobjects.findall("specobject"):
            obj_id = specobject.findtext("id")
            version = specobject.findtext("version")
            shallow_status = specobject.findtext("coverage/shallowCoverageStatus")
            # Extract uncoveredTypes as a comma-separated string
            uncovered_types_elem = specobject.find("coverage/uncoveredTypes")
            if uncovered_types_elem is not None:
                uncovered_types = [ut.text for ut in uncovered_types_elem.findall("uncoveredType") if ut.text]
                uncovered_types_str = ",".join(uncovered_types)
            else:
                uncovered_types_str = ""
            key = (doctype, obj_id, version)
            result[key] = (shallow_status, uncovered_types_str)
    return result

def main(file_a, file_b):
    a_objs = extract_specobjects(file_a)
    b_objs = extract_specobjects(file_b)
    for key, (b_status, b_uncovered_types) in b_objs.items():
        if b_status == "UNCOVERED":
            a_status = a_objs.get(key, (None, ""))[0]
            if a_status == "COVERED" or a_status is None:
                line = f"{key[0]}~{key[1]}~{key[2]}"
                if b_uncovered_types:
                    line += f": uncovered {b_uncovered_types}"
                print(line)

if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("Usage: python compare_req_tracing.py <old-report.xml> <new-report.xml>")
        sys.exit(1)
    main(sys.argv[1], sys.argv[2])
