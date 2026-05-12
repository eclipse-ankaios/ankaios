# Maintainer: Christoph Hamm <christoph.hamm@elektrobit.com>

pkgbase=ankaios
pkgname=(ankaios-server ankaios-agent ankaios-cli ankaios)
groups=(ankaios)
pkgver=ANKAIOS_VERSION
pkgrel=1
arch=('x86_64' 'aarch64')
url="https://eclipse-ankaios.github.io/ankaios"
license=('Apache-2.0')
depends=('libgcc' 'glibc')
makedepends=('cargo' 'protobuf' 'help2man')
source=("$pkgbase-$pkgver.tar.gz::https://github.com/eclipse-ankaios/ankaios/archive/refs/tags/v$pkgver.tar.gz"
	'ank-server.service'
	'ank-agent.service')
b2sums=('xxxxxx'
        'xxxxxx'
        'xxxxxx')

prepare() {
    cd "ankaios-$pkgver"
    export RUSTUP_TOOLCHAIN=stable
    cargo fetch --locked --target host-tuple
}

build() {
    cd "ankaios-$pkgver"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    export CFLAGS+=" -ffat-lto-objects"
    cargo build --frozen --release --target host-tuple
    ./tools/generate_man_pages.sh "target/$(rustc --print host-tuple)/release/" build/man/
}

package_ankaios-server() {
    pkgdesc="The server application of Eclipse Ankaios"
    backup=("etc/ankaios/ank-server.conf"
            "etc/ankaios/state.yaml")

    install -Dm755 -t "$pkgdir"/usr/bin/ "ankaios-$pkgver/target/$(rustc --print host-tuple)/release/ank-server"
    install -Dm644 -t "$pkgdir"/usr/lib/systemd/system/ ank-server.service
    install -Dm644 -t "$pkgdir"/etc/ankaios/ "ankaios-$pkgver"/server/config/ank-server.conf
    install -Dm644 -t "$pkgdir"/etc/ankaios/ "ankaios-$pkgver"/server/config/state.yaml
    install -Dm644 -t "$pkgdir"/usr/share/man/man8 "ankaios-$pkgver"/build/man/man8/ank-server.8
}

package_ankaios-agent() {
    pkgdesc="An agent running on each node of an Eclipse Ankaios cluster"
    backup=("etc/ankaios/ank-agent.conf")
    optdepends=(
      'podman: for running podman workloads'
      'nerdctl: for running containerd workloads'
    )

    install -Dm755 -t "$pkgdir"/usr/bin/ "ankaios-$pkgver/target/$(rustc --print host-tuple)/release/ank-agent"
    install -Dm644 -t "$pkgdir"/usr/lib/systemd/system/ ank-agent.service
    install -Dm644 -t "$pkgdir"/etc/ankaios/ "ankaios-$pkgver"/agent/config/ank-agent.conf
    install -Dm644 -t "$pkgdir"/usr/share/man/man8 "ankaios-$pkgver"/build/man/man8/ank-agent.8
}

package_ankaios-cli() {
    pkgdesc="A command line tool for communicating with the API of the Eclipse Ankaios server"
    install=ankaios-cli.install
    backup=("etc/ankaios/ank.conf")

    install -Dm755 -t "$pkgdir"/usr/bin/ "ankaios-$pkgver/target/$(rustc --print host-tuple)/release/ank"
    install -Dm644 -t "$pkgdir"/etc/ankaios/ "ankaios-$pkgver"/ank/config/ank.conf
    install -Dm644 -t "$pkgdir"/usr/share/man/man1 "ankaios-$pkgver"/build/man/man1/*
}

package_ankaios() {
    pkgdesc="Meta-package to install all components of Eclipse Ankaios"
    depends=('ankaios-server' 'ankaios-agent' 'ankaios-cli')
}
