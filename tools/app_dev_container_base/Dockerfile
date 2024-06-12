FROM docker.io/ubuntu:22.04

# example version: ANKAIOS_VERSION=0.1.0, if not provided latest is used
ARG ANKAIOS_VERSION
ARG TARGETARCH

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
    && rm -rf /var/lib/apt/lists/*

# install grpcurl for debugging purposes
RUN if [ "$TARGETARCH" = "amd64" ]; then \
        ITEMARCH="x86_64"; \
    elif [ "$TARGETARCH" = "arm64" ]; then \
        ITEMARCH="arm64"; \
    else \
        exit 1; \
    fi; \
    mkdir /usr/bin/grpcurl.d \
        && curl -sSL https://github.com/fullstorydev/grpcurl/releases/download/v1.8.7/grpcurl_1.8.7_linux_${ITEMARCH}.tar.gz | tar -xvz --directory /usr/bin/grpcurl.d \
        && ln /usr/bin/grpcurl.d/grpcurl /usr/bin/grpcurl

RUN useradd ankaios; \
echo -e "ankaios:1:999\nankaios:1001:64535" > /etc/subuid; \
echo -e "ankaios:1:999\nankaios:1001:64535" > /etc/subgid;

COPY containers.conf /etc/containers/containers.conf
COPY podman-containers.conf /home/ankaios/.config/containers/containers.conf

RUN mkdir -p /home/ankaios/.local/share/containers && \
    chown ankaios:ankaios -R /home/ankaios && \
    chmod 644 /etc/containers/containers.conf

# Copy & modify the defaults to provide reference if runtime changes needed.
# Changes here are required for running with fuse-overlay storage inside container.
RUN sed -e 's|^#mount_program|mount_program|g' \
           -e '/additionalimage.*/a "/var/lib/shared",' \
           -e 's|^mountopt[[:space:]]*=.*$|mountopt = "nodev,fsync=0"|g' \
           /usr/share/containers/storage.conf \
           > /etc/containers/storage.conf

# Setup internal Podman to pass subscriptions down from host to internal container
RUN printf '/run/secrets/etc-pki-entitlement:/run/secrets/etc-pki-entitlement\n/run/secrets/rhsm:/run/secrets/rhsm\n' > /etc/containers/mounts.conf

# Note VOLUME options must always happen after the chown call above
# RUN commands can not modify existing volumes
VOLUME /var/lib/containers
VOLUME /home/ankaios/.local/share/containers

RUN mkdir -p /var/lib/shared/overlay-images \
             /var/lib/shared/overlay-layers \
             /var/lib/shared/vfs-images \
             /var/lib/shared/vfs-layers && \
    touch /var/lib/shared/overlay-images/images.lock && \
    touch /var/lib/shared/overlay-layers/layers.lock && \
    touch /var/lib/shared/vfs-images/images.lock && \
    touch /var/lib/shared/vfs-layers/layers.lock

RUN mkdir -p /workspaces/

# Download and install latest Ankaios release
RUN if [ -n "$ANKAIOS_VERSION" ] ; then curl -sfL https://github.com/eclipse-ankaios/ankaios/releases/download/${ANKAIOS_VERSION}/install.sh | bash -s -- -v ${ANKAIOS_VERSION} ; curl -sL https://github.com/eclipse-ankaios/ankaios/releases/download/${ANKAIOS_VERSION}/{ank_base,control_api}.proto --create-dirs -O --output-dir /usr/local/lib/ankaios/proto ; else curl -sfL https://github.com/eclipse-ankaios/ankaios/releases/latest/download/install.sh | bash -; curl -sL https://github.com/eclipse-ankaios/ankaios/releases/latest/download/{ank_base,control_api}.proto --create-dirs -O --output-dir /usr/local/lib/ankaios/proto ; fi

