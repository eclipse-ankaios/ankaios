# Arch Linux (AUR) Packaging

This folder contains scripts and templates for generating and publishing Ankaios AUR packages.

Three package variants are currently maintained for AUR packaging:

- `ankaios` (stable release from source)
- `ankaios-bin` (prebuilt release binaries)
- `ankaios-git` (latest git revision)

These scripts are currently not executed by CI/CD.
They are intended to be run manually by the package maintainer outside the dev container.

| File | Purpose |
| ------ | --------- |
| `build_package.sh` | Generates `PKGBUILD` from a template, resolves checksums, and generates `.SRCINFO` for a single package variant into a given output directory. Does not touch AUR |
| `publish_to_aur.sh` | Clones AUR repositories, calls `build_package.sh` for each package variant, commits, and pushes changes |
| `PKGBUILD-ankaios.m4` | Template for the release-from-source AUR package |
| `PKGBUILD-ankaios-bin.m4` | Template for the prebuilt-binary AUR package |
| `PKGBUILD-ankaios-git.m4` | Template for the git-head AUR package |
| `ank-server.service`, `ank-agent.service`, `ankaios-cli.install`, `ankaios.sysusers` | Asset files copied into each AUR package repository |
| `ankaios.sysusers` | `systemd-sysusers` fragment creating the system group `ankaios` used for the server's Unix domain socket (`/run/ankaios/server.sock`), installed to `/usr/lib/sysusers.d/ankaios.conf` |

## Required environment

- `ANKAIOS_VERSION` must be set (for example `1.0.0`)
- SSH access to AUR (`aur@aur.archlinux.org`) must be configured, including the correct SSH key for the AUR maintainer account being available and loaded (only needed for `publish_to_aur.sh`)
- Required tools must be available: `m4`, `updpkgsums`, `makepkg`, `git`. Install them with `sudo pacman -S m4 pacman-contrib git`.

## Manual invocation

```bash
ANKAIOS_VERSION=1.0.0 ./publish_to_aur.sh
```

## Building a package locally (no AUR access)

Use `build_package.sh` directly to generate `PKGBUILD`/`.SRCINFO` and resolve checksums for a single package variant, without cloning from or pushing to AUR.
This does not require SSH access to AUR, but `updpkgsums` and `makepkg` still require an Arch (or Arch-compatible) environment:

```bash
ANKAIOS_VERSION=v1.0.0 ./build_package.sh ankaios /tmp/ankaios-build
cd /tmp/ankaios-build
makepkg -s
```

## Useful links

- [AUR submission guidelines](https://wiki.archlinux.org/title/AUR_submission_guidelines)
- [Creating packages (PKGBUILD)](https://wiki.archlinux.org/title/Creating_packages)
- [PKGBUILD reference](https://man.archlinux.org/man/PKGBUILD.5)
- [AUR package guidelines](https://wiki.archlinux.org/title/AUR_package_guidelines)
