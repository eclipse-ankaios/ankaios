#!/bin/bash

apt update
apt install -y software-properties-common uidmap tmux
sh -c "echo 'deb http://download.opensuse.org/repositories/devel:/kubic:/libcontainers:/stable/xUbuntu_20.04/ /' > /etc/apt/sources.list.d/devel:kubic:libcontainers:stable.list"
apt-key adv --keyserver hkp://keyserver.ubuntu.com:80 --recv-keys 4D64390375060AA4
apt update
apt install -y podman
