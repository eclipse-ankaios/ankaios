FROM docker.io/rust:slim as compile

ARG WORKSPACE_DIR=/workspaces/build

RUN apt-get update && apt-get install -y \
    # Development tools
    build-essential \
    pkg-config \
    protobuf-compiler \
    libprotobuf-dev \
    && rm -rf /var/lib/apt/lists/*

COPY ankaios_api ${WORKSPACE_DIR}/ankaios_api
COPY spec_macros ${WORKSPACE_DIR}/spec_macros
COPY examples/rust_control_interface ${WORKSPACE_DIR}
WORKDIR ${WORKSPACE_DIR}
ENV ANKAIOS_VERSION=1.0.0
RUN --mount=type=cache,target=${WORKSPACE_DIR}/target/release cargo build --release \
    && cp ${WORKSPACE_DIR}/target/release/control_interface_example /usr/local/bin/

# stage prod
FROM docker.io/rust:slim
COPY --from=compile /usr/local/bin/control_interface_example /usr/local/bin/control_interface_example
RUN chmod +x /usr/local/bin/control_interface_example

ENTRYPOINT ["/usr/local/bin/control_interface_example"]
