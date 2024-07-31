FROM ubuntu:24.04

ARG TARGETARCH
ARG VERSION=v0.4.0-rc1

RUN apt update && \
    apt install -y curl podman

COPY containers.conf /etc/containers/containers.conf

RUN curl -sfL https://github.com/eclipse-ankaios/ankaios/releases/download/${VERSION}/ankaios-linux-${TARGETARCH}.tar.gz | tar xz -C /usr/local/bin

CMD ["/usr/local/bin/ank-agent", "--insecure", "--name", "agent_A", "--server-url", "http://ank-server:25551"]
