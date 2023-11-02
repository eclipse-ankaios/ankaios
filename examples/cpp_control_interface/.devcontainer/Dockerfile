FROM ghcr.io/eclipse-ankaios/app-ankaios-dev:latest as dev


RUN apt-get update && apt-get -y install \
    # Development tools
    build-essential \
    cmake \
    && \
    rm -rf /var/lib/apt/lists/*

# RUN protoc --python_out=/usr/local/lib/ankaios/ --proto_path=/usr/local/lib/ankaios/ ankaios.proto

FROM dev as compile
COPY ./ /workspaces/app
RUN cd /workspaces/app && mkdir build && cd build && cmake .. && make

# prod stage
#FROM docker.io/library/alpine:latest
FROM docker.io/library/ubuntu:22.04
COPY --from=compile /workspaces/app/build/main /usr/local/bin/control_interface
COPY --from=compile /lib/x86_64-linux-gnu/libprotobuf.so.23 /usr/lib
RUN chmod +x /usr/local/bin/control_interface && chmod +x /usr/lib/libprotobuf.so.23

ENTRYPOINT ["/usr/local/bin/control_interface"]