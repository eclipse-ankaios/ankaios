FROM docker.io/ubuntu:latest as dev

RUN apt-get update && apt-get -y install \
    # Development tools
    protobuf-compiler \
    protobuf-compiler-grpc \
    build-essential \
    cmake \
    && \
    rm -rf /var/lib/apt/lists/*

FROM dev as compile
COPY api/proto/ankaios.proto /usr/local/lib/ankaios/ankaios.proto
COPY examples/cpp_control_interface /workspaces/app
RUN cd /workspaces/app && mkdir build && cd build && cmake .. && make

# prod stage
FROM docker.io/library/ubuntu:22.04
COPY --from=compile /workspaces/app/build/main /usr/local/bin/control_interface_example
COPY --from=compile /lib/x86_64-linux-gnu/libprotobuf.so.23 /usr/lib
RUN chmod +x /usr/local/bin/control_interface_example && chmod +x /usr/lib/libprotobuf.so.23

ENTRYPOINT ["/usr/local/bin/control_interface_example"]
