#!/bin/bash

# Check if the correct number of arguments is provided
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "Usage: $0 <filename> <repeat count> [image name]"
  echo "If no image name is provided, the default image 'localhost/minimal-hello-world' will be used."
  exit 1
fi

# Assign arguments to variables
FILENAME=$1
REPEAT_COUNT=$2

if [ -n "$3" ]; then
  IMAGE=$3
else
  IMAGE="localhost/minimal-hello-world"
fi


# Create or empty the file with the required header
printf "apiVersion: v0.1\nworkloads:\n" > $FILENAME

# Loop to append the content to the file
for ((i=1; i<=REPEAT_COUNT; i++))
do
cat <<EOT >> $FILENAME
  workload_$i:
    runtime: podman
    agent: X
    runtimeConfig: |
      image: $IMAGE
EOT

done

echo "File '$FILENAME' created with $REPEAT_COUNT lines of content."
