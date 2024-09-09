#!/bin/bash

COPYRIGHT_HEADER="// Copyright (c)"
FILE_REG="*.rs"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SEARCH_DIR="$SCRIPT_DIR/.."

eval "find \"$SEARCH_DIR\" -name \"$FILE_REG\" -print" | while read -r file; do
    if ! grep -q "$COPYRIGHT_HEADER" "$file"; then
        echo "Missing copyright header in: $file"
    fi
done
