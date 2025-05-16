#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SEARCH_DIR="$SCRIPT_DIR/.."

COPYRIGHT_REGEX="^.{1,3}[[:space:]]Copyright[[:space:]]\\(c\\)[[:space:]]20+[0-9]{2}"
# List of extensions to search for
FILE_EXT=(
    "*.rs"
)
# Exclude paths from search
EXCLUDE_PATHS=(
    "$SEARCH_DIR/target/*"
    "$SEARCH_DIR/vendor/*"
)

function check_copyright_headers() {
    local file_ext_cmd=""
    file_ext_cmd=$(printf " -o -name '%s'" "${FILE_EXT[@]}")
    file_ext_cmd="${file_ext_cmd:4}"  # Remove the leading ' -o'

    local exclude_paths_cmd=""
    for path in "${EXCLUDE_PATHS[@]}"; do
        exclude_paths_cmd="$exclude_paths_cmd -not -path '$path'"
    done

    local cmd="find \"$SEARCH_DIR\" -type f \\( $file_ext_cmd \\) $exclude_paths_cmd"
    missing_files=$(eval "$cmd" | xargs grep -L -E "$COPYRIGHT_REGEX")

    if [ -n "$missing_files" ]; then
        echo "No copyright header found in:"
        echo "$missing_files"
        no_copyright_count=$(echo "$missing_files" | wc -l)
        echo "Total files without copyright header: $no_copyright_count"
        return 1
    else
        echo "All files have copyright headers."
        return 0
    fi
}

check_copyright_headers
