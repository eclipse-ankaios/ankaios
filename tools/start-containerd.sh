#!/bin/bash
set -e

script_dir=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

LOG_DIR="$script_dir/../logs"
LOG_FILE="$LOG_DIR/containerd.log"

if [ -n "$1" ]; then
    LOG_FILE="$1"
    echo "Log file set to: $LOG_FILE"
fi

# create log directory
mkdir -p "$(dirname "$LOG_FILE")"

sudo containerd 2>&1 | sudo tee "$LOG_FILE" &
