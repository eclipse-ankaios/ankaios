FROM ubuntu:24.04

ARG TARGETARCH
ARG VERSION=vv0.4.0-rc1

RUN apt update && \
    apt install -y curl

RUN curl -sfL https://github.com/eclipse-ankaios/ankaios/releases/download/${VERSION}/ankaios-linux-${TARGETARCH}.tar.gz | tar xz -C /usr/local/bin

COPY state.yaml /etc/ankaios/state.yaml

CMD ["/usr/local/bin/ank-server", "--insecure", "--address", "0.0.0.0:25551", "--startup-config", "/etc/ankaios/state.yaml"]
