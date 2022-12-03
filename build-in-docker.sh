#!/bin/bash -e
mkdir -p ./target/docker_build/target ./target/docker_build/cargo_registry
docker run --rm --user "$(id -u)":"$(id -g)"\
    -v "$PWD":/usr/src/myapp\
    -v "$PWD"/target/docker_build/target:/usr/src/myapp/target:rw\
    -v "$PWD"/target/docker_build/cargo_registry:/usr/local/cargo/registry\
    -v /home/dima/vf-openlimits:/usr/src/vf-openlimits:ro\
    -w /usr/src/myapp ami-rust:latest /bin/bash -c "rustc --version && cargo build --release"

# Stripping is not used to save stacktrace
#strip ./target/docker_build/target/release/hedged_staking
