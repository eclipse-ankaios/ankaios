FROM docker.io/rust:slim as compile

ARG WORKSPACE_DIR=/workspaces/build

RUN apt-get update && apt-get install -y \
    # Development tools
    git \
    build-essential \
    pkg-config \
    protobuf-compiler \
    libprotobuf-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR ${WORKSPACE_DIR}/ankaios_example
ENV ANKAIOS_VERSION=1.0.0

COPY examples/rust_sdk_logging ${WORKSPACE_DIR}/ankaios_example
COPY examples/tools/setup_rust_sdk.sh .

RUN if [[ -z "$RUST_SDK_BRANCH" ]] ; then ./setup_rust_sdk.sh --sdk-source crates --sdk-version "$ANKAIOS_VERSION" ; else ./setup_rust_sdk.sh --sdk-source github --sdk-branch "$RUST_SDK_BRANCH" ; fi

RUN --mount=type=cache,target=${WORKSPACE_DIR}/target/release cargo build --release \
    && cp ${WORKSPACE_DIR}/ankaios_example/target/release/rust_sdk_logging /usr/local/bin/rust_sdk_logging

ENV RUST_BACKTRACE=full
ENV RUST_LOG=trace

ENTRYPOINT ["/usr/local/bin/rust_sdk_logging"]
