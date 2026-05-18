# Arch Linux (AUR) Packaging

This folder contains scripts and templates for generating and publishing Ankaios AUR packages.

Three package variants are currently maintained for AUR packaging:

- `ankaios` (stable release from source)
- `ankaios-bin` (prebuilt release binaries)
- `ankaios-git` (latest git revision)

These scripts are currently not executed by CI/CD.
They are intended to be run manually by the package maintainer.

| File | Purpose |
| ------ | --------- |
| `publish_to_aur.sh` | Clones AUR repositories, generates `PKGBUILD` from templates, updates checksums and `.SRCINFO`, commits, and pushes changes |
| `PKGBUILD-ankaios.m4` | Template for the release-from-source AUR package |
| `PKGBUILD-ankaios-bin.m4` | Template for the prebuilt-binary AUR package |
| `PKGBUILD-ankaios-git.m4` | Template for the git-head AUR package |
| `ank-server.service`, `ank-agent.service`, `ankaios-cli.install` | Asset files copied into each AUR package repository |

## Required environment

- `ANKAIOS_VERSION` must be set (for example `1.0.0`)
- SSH access to AUR (`aur@aur.archlinux.org`) must be configured, including the correct SSH key for the AUR maintainer account being available and loaded
- Required tools must be available: `m4`, `updpkgsums`, `makepkg`, `git`

## Manual invocation

```bash
ANKAIOS_VERSION=v1.0.0 ./publish_to_aur.sh
```

## Useful links

- [AUR submission guidelines](https://wiki.archlinux.org/title/AUR_submission_guidelines)
- [Creating packages (PKGBUILD)](https://wiki.archlinux.org/title/Creating_packages)
- [PKGBUILD reference](https://man.archlinux.org/man/PKGBUILD.5)
- [AUR package guidelines](https://wiki.archlinux.org/title/AUR_package_guidelines)
