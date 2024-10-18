FROM docker.io/alpine:3.18.4 as compile

RUN apk update && apk add --update-cache \
    # Development tools
    protobuf \
    protobuf-dev \
    protoc \
    build-base \
    cmake \
    && rm -rf /var/cache/apk/*

COPY api/proto/ank_base.proto api/proto/control_api.proto /usr/local/lib/ankaios/
COPY examples/cpp_control_interface /workspaces/app
RUN cd /workspaces/app && mkdir build && cd build && cmake .. && make

# prod stage
FROM docker.io/library/alpine:3.18.4
RUN apk update && apk add --update-cache protobuf-dev
COPY --from=compile /workspaces/app/build/main /usr/local/bin/control_interface_example
RUN chmod +x /usr/local/bin/control_interface_example
ENV ANKAIOS_VERSION=0.5.0-pre

ENTRYPOINT ["/usr/local/bin/control_interface_example"]
