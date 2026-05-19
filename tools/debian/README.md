# Debian Packaging

This folder contains scripts for building and publishing Debian packages. Built on Ubuntu 22.04 (jammy), the packages are compatible with Ubuntu 22.04+, Debian 12+ and other distributions which have at least glibc 2.35.

These scripts are meant to be called only from the CI/CD. Running them locally might not work properly.

| Script                | Purpose                                                                                         |
|-----------------------|-------------------------------------------------------------------------------------------------|
| `build_src_pkg.sh`    | Writes the `debian/` tree and runs `dpkg-buildpackage` to produce the source package in `dist/` |
| `build_bin_pkgs.sh`   | Extracts the source package and builds binary `.deb` files into `dist/`                         |
| `publish_bin_pkgs.sh` | Uploads binary `.deb` files from `dist/` to the Nexus apt repository                            |
| `delete_pkg.py`       | Searches for and deletes packages from the Nexus apt repository                                 |

## Linting

```bash
lintian dist/ank-server_<version>_amd64.deb
```

## Useful links

- [Debian New Maintainers' Guide - debian.org](https://www.debian.org/doc/manuals/maint-guide/)
- [Create source package - debian.org](https://www.debian.org/doc/debian-policy/ch-controlfields.html#debian-source-package-template-control-files-debian-control)
