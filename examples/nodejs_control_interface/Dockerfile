FROM docker.io/alpine:3.18.4

RUN apk update && apk add --update-cache \
    # Development tools
    nodejs \
    npm \
    && rm -rf /var/cache/apk/*

COPY api/proto/ank_base.proto api/proto/control_api.proto /usr/local/lib/ankaios/
COPY examples/nodejs_control_interface /workspaces/app
WORKDIR /workspaces/app
RUN npm install
ENV ANKAIOS_VERSION=0.5.0-pre

ENTRYPOINT ["node", "/workspaces/app/src/main.js"]
