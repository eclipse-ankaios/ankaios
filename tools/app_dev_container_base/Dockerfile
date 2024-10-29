FROM ubuntu:24.04

# example version: ANKAIOS_VERSION=0.1.0, if not provided latest is used
ARG ANKAIOS_VERSION
ARG TARGETARCH
ARG USERNAME=ankaios

RUN export DEBIAN_FRONTEND=noninteractive \
    && apt update \
    && apt -y install \
    sudo \
    # Protobuf
    protobuf-compiler \
    protobuf-compiler-grpc \
    # Others
    gpg \
    curl \
    libssl-dev \
    tmux \
    vim \
    uidmap \
    fuse3 \
    fuse-overlayfs \
    slirp4netns \
    # podman
    podman \
    && rm -rf /var/lib/apt/lists/*

# Workaround for podman not being able to stop containers, see https://bugs.launchpad.net/ubuntu/noble/+source/libpod/+bug/2040483
RUN mkdir -p /etc/containers/containers.conf.d \
    && printf '[CONTAINERS]\napparmor_profile=""\n' > /etc/containers/containers.conf.d/disable-apparmor.conf

# User management
RUN (userdel -r ubuntu || true) \
    && useradd -s /bin/bash -d /home/${USERNAME} -m ${USERNAME} \
    && echo "${USERNAME} ALL=(root) NOPASSWD:ALL" > /etc/sudoers.d/${USERNAME}

# Prepare shells
USER ${USERNAME}
COPY --chown=${USERNAME}:${USERNAME} dot_bashrc /home/${USERNAME}/.bashrc
COPY --chown=${USERNAME}:${USERNAME} dot_zshrc /home/${USERNAME}/.zshrc
COPY --chown=${USERNAME}:${USERNAME} dot_tmux.conf /home/${USERNAME}/.tmux.conf
RUN curl -sS https://starship.rs/install.sh | sh -s -- -y \
    && echo 'eval "$(starship init bash)"' >> /home/${USERNAME}/.bashrc \
    && echo 'eval "$(starship init zsh)"' >> /home/${USERNAME}/.zshrc
COPY --chown=${USERNAME}:${USERNAME} starship.toml /home/${USERNAME}/.config/
USER root

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

COPY containers.conf /etc/containers/containers.conf
COPY podman-containers.conf /home/${USERNAME}/.config/containers/containers.conf

RUN mkdir -p /home/${USERNAME}/.local/share/containers && \
    chown ${USERNAME}:${USERNAME} -R /home/${USERNAME} && \
    chmod 644 /etc/containers/containers.conf

# Note VOLUME options must always happen after the chown call above
# RUN commands can not modify existing volumes
VOLUME /var/lib/containers
VOLUME /home/ankaios/.local/share/containers

RUN mkdir -p /workspaces/

# Download and install latest Ankaios release
RUN if [ -n "$ANKAIOS_VERSION" ] ; then curl -sfL https://github.com/eclipse-ankaios/ankaios/releases/download/${ANKAIOS_VERSION}/install.sh | bash -s -- -v ${ANKAIOS_VERSION} ; curl -sL https://github.com/eclipse-ankaios/ankaios/releases/download/${ANKAIOS_VERSION}/{ank_base,control_api}.proto --create-dirs -O --output-dir /usr/local/lib/ankaios/proto ; else curl -sfL https://github.com/eclipse-ankaios/ankaios/releases/latest/download/install.sh | bash -; curl -sL https://github.com/eclipse-ankaios/ankaios/releases/latest/download/{ank_base,control_api}.proto --create-dirs -O --output-dir /usr/local/lib/ankaios/proto ; fi
