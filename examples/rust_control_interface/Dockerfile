FROM docker.io/ubuntu:latest as dev
ENV PATH="/root/.cargo/bin:${PATH}"

RUN apt-get update && apt-get -y install \
    curl \
    # Development tools
    protobuf-compiler \
    protobuf-compiler-grpc \
    musl \
    musl-dev \
    musl-tools\
    && curl --proto '=https' --tlsv1.2 -sS https://sh.rustup.rs | bash -s -- --target x86_64-unknown-linux-musl -y > /dev/null \
    && rm -rf /var/lib/apt/lists/*

# stage compile
FROM dev as compile
COPY api /workspaces/build/api
COPY examples/rust_control_interface /workspaces/build
WORKDIR /workspaces/build
RUN cargo build --release

# stage prod
FROM docker.io/library/alpine:latest
COPY --from=compile /workspaces/build/target/x86_64-unknown-linux-musl/release/control_interface_example /usr/local/bin/control_interface_example
RUN chmod +x /usr/local/bin/control_interface_example
ENTRYPOINT ["/usr/local/bin/control_interface_example"]
