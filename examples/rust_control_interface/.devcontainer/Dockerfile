FROM ghcr.io/eclipse-ankaios/app-ankaios-dev:latest as dev

RUN apt-get update && apt-get -y install \
    # Development tools
    gcc \
    && \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash /dev/stdin -y \
    && \
    rm -rf /var/lib/apt/lists/*

# prod stage
FROM docker.io/library/rust:1.72-slim-bullseye
RUN apt-get update && apt-get -y install \
    protobuf-compiler \
    protobuf-compiler-grpc

# copy ankaios.proto
COPY --from=dev /usr/local/lib/ankaios /usr/local/lib/ankaios
COPY . /workspaces/app
WORKDIR /workspaces/app
RUN cargo build -r

ENTRYPOINT ["/workspaces/app/target/release/rust_control_interface_example"]