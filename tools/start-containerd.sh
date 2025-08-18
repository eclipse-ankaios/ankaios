#!/bin/bash
set -e

script_dir=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

LOG_DIR="$script_dir/../logs"
LOG_FILE="$LOG_DIR/containerd.log"

# Ensure log directory exists
mkdir -p "$LOG_DIR"

# Start containerd with sudo and log output
sudo containerd 2>&1 | sudo tee "$LOG_FILE" &
