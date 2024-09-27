#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SEARCH_DIR="$SCRIPT_DIR/.."

COPYRIGHT_REGEX="Copyright[[:space:]]\\(c\\)[[:space:]]20+[0-9]{2}"
# List of extensions to search for
FILE_EXT=(
    "*.rs"
)
# Exclude paths from search
EXCLUDE_PATHS=(
    "$SEARCH_DIR/target/*"
)

function check_copyright_headers() {
    local file_ext_cmd=""
    for ext in "${FILE_EXT[@]}"; do
        if [ -n "$file_ext_cmd" ]; then
            file_ext_cmd="$file_ext_cmd -o -name \"$ext\""
        else
            file_ext_cmd="-name \"$ext\""
        fi
    done

    local exclude_paths_cmd=""
    for path in "${EXCLUDE_PATHS[@]}"; do
        exclude_paths_cmd="$exclude_paths_cmd -not -path \"$path\""
    done

    local cmd="find \"$SEARCH_DIR\" -type f $file_ext_cmd $exclude_paths_cmd -print"
    local no_copyright_count=0

    while IFS= read -r file; do
        # Check if the copyright header is present in the file
        if ! grep -E -q "$COPYRIGHT_REGEX" "$file"; then
            echo "No copyright header found in: $file"
            no_copyright_count=$((no_copyright_count + 1))
        fi
    done < <(eval "$cmd")

    if [ $no_copyright_count -gt 0 ]; then
        echo "Total files without copyright header: $no_copyright_count"
        return 1
    fi

    echo "All files have copyright headers."
    return 0
}

check_copyright_headers
RESULT=$?

exit $RESULT
