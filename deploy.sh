#!/bin/bash -e
./build-in-docker.sh
scp target/docker_build/target/release/hedged_staking vfh:~/
scp misc/eth-operations.py vfh:~/misc/
