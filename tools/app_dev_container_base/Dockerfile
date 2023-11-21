FROM docker.io/ubuntu:22.04

# example version: ANKAIOS_VERSION=0.1.0, if not provided latest is used
ARG ANKAIOS_VERSION

RUN apt-get update && apt-get -y install \
    # Protobuf
    protobuf-compiler \
    protobuf-compiler-grpc \
    # Others
    gpg \
    curl \
    libssl-dev \
    # install podman 4
    && echo "deb https://download.opensuse.org/repositories/devel:/kubic:/libcontainers:/unstable/xUbuntu_22.04 /" | tee /etc/apt/sources.list.d/devel:kubic:libcontainers:unstable.list \
    && curl -fsSL https://download.opensuse.org/repositories/devel:/kubic:/libcontainers:/unstable/xUbuntu_22.04/Release.key | gpg --dearmor | tee /etc/apt/trusted.gpg.d/devel_kubic_libcontainers_unstable.gpg > /dev/null \
    && apt update \
    && apt install -y podman \
    && rm -rf /var/lib/apt/lists/* \
    # install grpcurl for gRPC debugging
    && curl -sL https://github.com/fullstorydev/grpcurl/releases/download/v1.8.7/grpcurl_1.8.7_linux_x86_64.tar.gz | tar xz --directory=/usr/local/bin grpcurl

VOLUME /var/lib/containers

COPY containers.conf /etc/containers/containers.conf
RUN chmod 644 /etc/containers/containers.conf

# Download and install latest Ankaios release
RUN if [ -n "$ANKAIOS_VERSION" ] ; then curl -sfL https://github.com/eclipse-ankaios/ankaios/releases/download/${ANKAIOS_VERSION}/install.sh | bash -s -- -v ${ANKAIOS_VERSION} ; else curl -sfL https://github.com/eclipse-ankaios/ankaios/releases/latest/download/install.sh | bash - ; fi
ADD https://github.com/eclipse-ankaios/ankaios/releases/latest/download/ankaios.proto /usr/local/lib/ankaios/ankaios.proto

# Mount point for local build of Ankaios
RUN mkdir -p /ankaios

# Mount point for user workspaces
RUN mkdir -p /workspaces/
