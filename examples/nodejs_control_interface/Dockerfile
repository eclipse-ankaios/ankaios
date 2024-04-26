FROM docker.io/alpine:3.18.4

RUN apk update && apk add --update-cache \
    # Development tools
    nodejs \
    npm \
    && rm -rf /var/cache/apk/*

COPY api/proto/ank_proto.proto api/proto/control_interface_api.proto /usr/local/lib/ankaios/
COPY examples/nodejs_control_interface /workspaces/app
WORKDIR /workspaces/app

RUN npm install

ENTRYPOINT ["node", "/workspaces/app/src/main.js"]
