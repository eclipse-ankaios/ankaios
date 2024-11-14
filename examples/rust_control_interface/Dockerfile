FROM docker.io/alpine:3.18.4 as compile
ENV PATH="/root/.cargo/bin:${PATH}"

ARG WORKSPACE_DIR=/workspaces/build

RUN apk update && apk add --update-cache \
    curl \
    # Development tools
    protobuf \
    protobuf-dev \
    protoc \
    build-base \
    && rm -rf /var/cache/apk/*

RUN curl --proto '=https' --tlsv1.2 -sS https://sh.rustup.rs | sh -s -- -y > /dev/null

COPY api ${WORKSPACE_DIR}/api
COPY examples/rust_control_interface ${WORKSPACE_DIR}
WORKDIR ${WORKSPACE_DIR}
ENV ANKAIOS_VERSION=0.5.0
RUN --mount=type=cache,target=${WORKSPACE_DIR}/target/release cargo build --release \
    && cp ${WORKSPACE_DIR}/target/release/control_interface_example /usr/local/bin/

# stage prod
FROM docker.io/alpine:3.18.4
COPY --from=compile /usr/local/bin/control_interface_example /usr/local/bin/control_interface_example
RUN chmod +x /usr/local/bin/control_interface_example

ENTRYPOINT ["/usr/local/bin/control_interface_example"]
