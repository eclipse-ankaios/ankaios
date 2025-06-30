FROM ghcr.io/eclipse-ankaios/devcontainer-base:0.10.3 AS compile

ARG WORKSPACE_DIR=/workspaces/build

WORKDIR ${WORKSPACE_DIR}
ARG SRC_HASH
ADD "https://www.random.org/cgi-bin/randbyte?nbytes=10&format=h" skipcache
RUN --mount=type=bind,target=${WORKSPACE_DIR} --mount=type=cache,id=control_interface_example_target,target=${WORKSPACE_DIR}/target cargo build --release --bin control_interface_tester \
    && cp ${WORKSPACE_DIR}/target/x86_64-unknown-linux-musl/release/control_interface_tester /usr/local/bin/

FROM docker.io/alpine:latest
COPY --from=compile /usr/local/bin/control_interface_tester /usr/local/bin/control_interface_tester
RUN chmod +x /usr/local/bin/control_interface_tester
ENTRYPOINT ["/usr/local/bin/control_interface_tester"]
