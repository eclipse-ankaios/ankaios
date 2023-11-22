FROM docker.io/ubuntu:latest as dev

RUN apt-get update && apt-get -y install \
    # Development tools
    protobuf-compiler \
    protobuf-compiler-grpc \
    nodejs \
    npm \
    && \
    rm -rf /var/lib/apt/lists/*

# prod stage
FROM docker.io/library/node:21-alpine3.17
# copy ankaios.proto
COPY api/proto/ankaios.proto /usr/local/lib/ankaios
COPY examples/nodejs_control_interface /workspaces/app
WORKDIR /workspaces/app
RUN npm install

ENTRYPOINT ["node", "/workspaces/app/src/main.js"]
