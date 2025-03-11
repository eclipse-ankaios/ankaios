#!/bin/sh

sigterm_handler() { exit 0; }
trap sigterm_handler TERM
sleep 1000000 &
wait
