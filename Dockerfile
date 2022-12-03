FROM amazonlinux:2

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH \
    RUST_VERSION=1.63.0

RUN yum install -y gcc gcc-c++ openssl-devel gmp-devel gcc-c++ git make cmake3.x86_64 && yum clean all && rm -rf /var/cache/yum; \
    ln -s /usr/bin/cmake3 /usr/bin/cmake; \
    curl https://sh.rustup.rs -sSf | sh -s -- --no-modify-path --profile minimal --default-toolchain $RUST_VERSION -y; \
    chmod -R a+w $RUSTUP_HOME $CARGO_HOME; \
    rustup --version; \
    cargo --version; \
    rustc --version;

WORKDIR /volume
