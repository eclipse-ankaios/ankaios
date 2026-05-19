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

# Generates man pages for all Ankaios executables and ank subcommands.
#
# Usage: generate_man_pages.sh <binaries_dir> <output_dir>
#
#   binaries_dir   Directory containing ank, ank-server, ank-agent binaries
#   output_dir     Destination; man1/ and man8/ subdirs are created automatically
#
# Example:
#   ./tools/man/generate_man_pages.sh target/release dist/man

set -e

if [ "$#" -ne 2 ]; then
    echo "Usage: $0 <binaries_dir> <output_dir>" >&2
    exit 1
fi

BINARIES_DIR="$1"
OUTPUT_DIR="$2"

ANK="$BINARIES_DIR/ank"
ANK_SERVER="$BINARIES_DIR/ank-server"
ANK_AGENT="$BINARIES_DIR/ank-agent"

for bin in "$ANK" "$ANK_SERVER" "$ANK_AGENT"; do
    if [ ! -x "$bin" ]; then
        echo "Error: binary not found or not executable: $bin" >&2
        exit 1
    fi
done

mkdir -p "$OUTPUT_DIR/man1" "$OUTPUT_DIR/man8"

get_ank_subcommands() {
    "$ANK" --help | awk '
        /^Commands:/ { in_usage=1; next }
        /^[[:space:]]*$/ { in_usage=0 }
        in_usage && /^[[:space:]]+([a-z]+)[[:space:]]/ {
            if ($1 == "help") next
            print $1
        }'
}

has_sub() {
    local cmd="$1"
    "$ANK" "$cmd" --help 2>&1 | grep -q "Commands:"
}

# Discover subcommands at runtime
SUBCOMMANDS=$(get_ank_subcommands)

INCLUDE=$(mktemp --suffix=.h2m)
WRAPPER=$(mktemp)
trap 'rm -f "$INCLUDE" "$WRAPPER"' EXIT

# --- ank(1) ---
{
    echo "[see also]"
    for cmd in ${SUBCOMMANDS}; do
        echo ".BR ank-${cmd} (1),"
    done
} > "$INCLUDE"

help2man --no-info --section=1 --name="Ankaios CLI" \
    --include="$INCLUDE" \
    "$ANK" -o "$OUTPUT_DIR/man1/ank.1"
echo "Generated $OUTPUT_DIR/man1/ank.1"

# --- ank-server(8) ---
help2man --no-info --section=8 --name="Ankaios server" \
    "$ANK_SERVER" -o "$OUTPUT_DIR/man8/ank-server.8"
echo "Generated $OUTPUT_DIR/man8/ank-server.8"

# --- ank-agent(8) ---
help2man --no-info --section=8 --name="Ankaios agent" \
    "$ANK_AGENT" -o "$OUTPUT_DIR/man8/ank-agent.8"
echo "Generated $OUTPUT_DIR/man8/ank-agent.8"

# --- ank-<subcommand>(1) ---
for cmd in ${SUBCOMMANDS}; do
    # Wrapper script: help2man invokes the binary with --help / --version.
    # Redirect --help to "ank <subcommand> --help".
    # Patch --version to output "ank-<subcommand> <version>" so that help2man
    # uses the correct program name in the .TH header (e.g. ANK-GET, not ANK).
    cat > "$WRAPPER" << WRAPPER_EOF
#!/bin/bash
case "\$1" in
  --help)    "${ANK}" ${cmd} --help ;;
  --version) "${ANK}" --version | sed "s/^ank /ank-${cmd} /" ;;
  *)         exit 1 ;;
esac
WRAPPER_EOF
    chmod +x "$WRAPPER"

    # Build the include file
    if has_sub "$cmd"; then
        cat > "$INCLUDE" << INCLUDE_EOF
[notes]
For details on each command, run \fBank ${cmd}\fR \fI<command>\fR \fB\-\-help\fR.

[see also]
.BR ank (1)
INCLUDE_EOF
    else
        cat > "$INCLUDE" << INCLUDE_EOF
[see also]
.BR ank (1)
INCLUDE_EOF
    fi

    help2man --no-info --section=1 --name="Ankaios CLI - ${cmd}" \
        --include="$INCLUDE" \
        "$WRAPPER" -o "$OUTPUT_DIR/man1/ank-${cmd}.1"
    echo "Generated $OUTPUT_DIR/man1/ank-${cmd}.1"
done
