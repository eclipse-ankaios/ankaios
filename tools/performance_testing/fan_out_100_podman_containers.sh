#!/bin/bash

for _ in {1..100}
do
    podman run -d localhost/minimal-hello-world &
done
