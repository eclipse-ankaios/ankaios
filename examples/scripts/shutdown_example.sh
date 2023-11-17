#!/bin/bash

# Cleanup Ankaios ....
echo "Cleaning up Ankaios..."
pkill ank-agent
pkill ank-server
echo "OK."

# Cleanup podman
echo "Cleaning up podman..."
podman stop -a >/dev/null 2>&1
podman rm -a >/dev/null 2>&1
echo "OK."