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

WORKDIR ${WORKSPACE_DIR}
ARG SRC_HASH
RUN --mount=type=bind,target=${WORKSPACE_DIR} --mount=type=cache,id=control_interface_example_target,target=${WORKSPACE_DIR}/target cargo build --release --bin control_interface_tester \
    && cp ${WORKSPACE_DIR}/target/x86_64-unknown-linux-musl/release/control_interface_tester /usr/local/bin/

FROM docker.io/alpine:3.18.4
COPY --from=compile /usr/local/bin/control_interface_tester /usr/local/bin/control_interface_tester
RUN chmod +x /usr/local/bin/control_interface_tester
ENTRYPOINT ["/usr/local/bin/control_interface_tester"]
