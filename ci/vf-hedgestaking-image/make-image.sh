#!/bin/bash -e

DIR=ci/vf-hedgestaking-image

cp target/docker_build/target/release/hedged_staking $DIR
cp misc/eth-operations.py $DIR
cd $DIR
docker build -t vf-hedgedstaking .
rm hedged_staking
rm eth-operations.py
