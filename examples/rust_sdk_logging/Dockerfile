FROM docker.io/alpine:3.18.4 as compile
ENV PATH="/root/.cargo/bin:${PATH}"

ARG WORKSPACE_DIR=/workspaces/build

RUN apk update && apk add --update-cache \
    curl \
    # Development tools
    bash \
    git \
    protobuf \
    protobuf-dev \
    protoc \
    build-base \
    && rm -rf /var/cache/apk/*

RUN curl --proto '=https' --tlsv1.2 -sS https://sh.rustup.rs | sh -s -- -y > /dev/null

WORKDIR ${WORKSPACE_DIR}/ankaios_example
ENV ANKAIOS_VERSION=0.5.0

COPY examples/rust_sdk_logging ${WORKSPACE_DIR}/ankaios_example
COPY examples/tools/setup_rust_sdk.sh .

RUN if [[ -z "$RUST_SDK_BRANCH" ]] ; then ./setup_rust_sdk.sh --sdk-source crates --sdk-version "$ANKAIOS_VERSION" ; else ./setup_rust_sdk.sh --sdk-source github --sdk-branch "$RUST_SDK_BRANCH" ; fi

RUN --mount=type=cache,target=${WORKSPACE_DIR}/target/release cargo build --release \
    && cp ${WORKSPACE_DIR}/ankaios_example/target/release/rust_sdk_logging /usr/local/bin/rust_sdk_logging

ENV RUST_BACKTRACE=full
ENV RUST_LOG=trace

ENTRYPOINT ["/usr/local/bin/rust_sdk_logging"]
