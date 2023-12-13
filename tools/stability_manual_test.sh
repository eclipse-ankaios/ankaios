#!/bin/bash

set -e -x

echo "Starting the test"

export RUST_BACKTRACE=1
export RUST_LOG=trace

function cleanup {
  echo "Terminating ank-server and ank-agents(s)"
  pkill -f ank-
  ps -a
}

trap cleanup EXIT

for i in {1..5}
do
  echo "Starting $i-th iteration"
  ./ank-server -c startConfig.yaml &

  sleep 10

  ./ank-agent --name agent_A &
  ./ank-agent --name agent_B &

  sleep 35

  ./ank get workloads
  sleep 5
  ./ank delete workload hello1
  sleep 10
  ./ank delete workload hello2
  sleep 10
  ./ank delete workload nginx
  sleep 10
  
  pkill -f ank-
  ps -a
  sleep 10

  echo "Done"
done
