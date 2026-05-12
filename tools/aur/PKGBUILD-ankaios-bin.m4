# Maintainer: Christoph Hamm <christoph.hamm@elektrobit.com>

pkgbase=ankaios-bin
pkgname=(ankaios-server-bin ankaios-agent-bin ankaios-cli-bin ankaios-bin)
groups=(ankaios-bin)
pkgver=ANKAIOS_VERSION
pkgrel=1
arch=('x86_64' 'aarch64')
url="https://eclipse-ankaios.github.io/ankaios"
license=('Apache-2.0')
depends=('libgcc' 'glibc')
source=("$pkgbase-${pkgver}_configs.tar.gz::https://github.com/eclipse-ankaios/ankaios/releases/download/v$pkgver/ankaios_configs.tar.gz"
        "$pkgbase-${pkgver}_man-pages.tar.gz::https://github.com/eclipse-ankaios/ankaios/releases/download/v$pkgver/man-pages.tar.gz"
	'ank-server.service'
	'ank-agent.service')
source_x86_64=("$pkgbase-$pkgver-x86_64.tar.gz::https://github.com/eclipse-ankaios/ankaios/releases/download/v$pkgver/ankaios-linux-amd64.tar.gz")
source_aarch64=("$pkgbase-$pkgver-aarch64.tar.gz::https://github.com/eclipse-ankaios/ankaios/releases/download/v$pkgver/ankaios-linux-arm64.tar.gz")
b2sums=('xxxxx'
        'xxxxx'
        'xxxxx'
        'xxxxx')
b2sums_x86_64=('xxxxx')
b2sums_aarch64=('xxxxx')

package_ankaios-server-bin() {
    pkgdesc="The server application of Eclipse Ankaios"
    provides=(ankaios-server)
    conflicts=(ankaios-server)
    backup=("etc/ankaios/ank-server.conf"
            "etc/ankaios/state.yaml")

    install -Dm755 -t "$pkgdir"/usr/bin/ "ank-server"
    install -Dm644 -t "$pkgdir"/usr/lib/systemd/system/ ank-server.service
    install -Dm644 -t "$pkgdir"/etc/ankaios/ ank-server.conf
    install -Dm644 -t "$pkgdir"/etc/ankaios/ state.yaml
    install -Dm644 -t "$pkgdir"/usr/share/man/man8 man8/ank-server.8
}

package_ankaios-agent-bin() {
    pkgdesc="An agent running on each node of an Eclipse Ankaios cluster"
    provides=(ankaios-agent)
    conflicts=(ankaios-agent)
    backup=("etc/ankaios/ank-agent.conf")
    optdepends=(
      'podman: for running podman workloads'
      'nerdctl: for running containerd workloads'
    )

    install -Dm755 -t "$pkgdir"/usr/bin/ ank-agent
    install -Dm644 -t "$pkgdir"/usr/lib/systemd/system/ ank-agent.service
    install -Dm644 -t "$pkgdir"/etc/ankaios/ ank-agent.conf
    install -Dm644 -t "$pkgdir"/usr/share/man/man8 man8/ank-agent.8
}

package_ankaios-cli-bin() {
    pkgdesc="A command line tool for communicating with the API of the Eclipse Ankaios server"
    provides=(ankaios-cli)
    conflicts=(ankaios-cli)
    install=ankaios-cli.install
    backup=("etc/ankaios/ank.conf")

    install -Dm755 -t "$pkgdir"/usr/bin/ ank
    install -Dm644 -t "$pkgdir"/etc/ankaios ank.conf
    install -Dm644 -t "$pkgdir"/usr/share/man/man1 man1/*
}

package_ankaios-bin() {
    pkgdesc="Meta-package to install all components of Eclipse Ankaios"
    depends=('ankaios-server-bin' 'ankaios-agent-bin' 'ankaios-cli-bin')
}
