FROM ghcr.io/eclipse-ankaios/app-ankaios-dev:latest as dev

RUN apt-get update && apt-get -y install \
    # Development tools
    nodejs \
    npm \
    && \
    rm -rf /var/lib/apt/lists/*

# prod stage
FROM docker.io/library/node:21-alpine3.17
# copy ankaios.proto
COPY --from=dev /usr/local/lib/ankaios /usr/local/lib/ankaios
COPY . /workspaces/app
WORKDIR /workspaces/app
RUN npm install

ENTRYPOINT ["node", "/workspaces/app/src/main.js"]
