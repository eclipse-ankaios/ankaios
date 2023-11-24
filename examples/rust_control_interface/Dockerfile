FROM docker.io/alpine:3.18.4 as compile
ENV PATH="/root/.cargo/bin:${PATH}"

RUN apk update && apk add --update-cache \
    curl \
    # Development tools
    protobuf \
    protobuf-dev \
    protoc \
    build-base \
    && curl --proto '=https' --tlsv1.2 -sS https://sh.rustup.rs | sh -s -- -y > /dev/null \
    && rm -rf /var/cache/apk/*

COPY api /workspaces/build/api
COPY examples/rust_control_interface /workspaces/build
WORKDIR /workspaces/build
RUN cargo build --release

# stage prod
FROM docker.io/alpine:3.18.4
COPY --from=compile /workspaces/build/target/release/control_interface_example /usr/local/bin/control_interface_example
RUN chmod +x /usr/local/bin/control_interface_example
ENTRYPOINT ["/usr/local/bin/control_interface_example"]
