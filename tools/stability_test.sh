#!/bin/bash
set -e

for i in {1..100}
do
    cargo test -- --show-output --nocapture
    echo -e "\033[35m$i-th\033[0m call finished"
    sleep 2
done
echo "The stability test has finished"
