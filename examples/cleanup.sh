#!/bin/bash

# Cleanup Ankaios ....
echo "Cleaning up Ankaios..."
pkill ank-agent
pkill ank-server
echo "done."

# Cleanup podman
if which podman > /dev/null; then
    echo "Cleaning up podman ..."
    podman stop -a >/dev/null 2>&1
    podman rm -a >/dev/null 2>&1
    podman volume rm -a >/dev/null 2>&1
    echo "done."
fi

# Cleanup containerd
if pgrep -x "containerd" > /dev/null; then
    echo "Cleaning up containerd ..."
    nerdctl stop "$(nerdctl ps -a -q)" >/dev/null 2>&1
    nerdctl rm "$(nerdctl ps -a -q)" >/dev/null 2>&1
    echo "done."
fi
