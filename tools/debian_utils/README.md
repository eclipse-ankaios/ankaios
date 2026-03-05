# Debian Packaging for Ankaios

This directory contains the tooling to create Debian **source packages** suitable for upload to a [LaunchPad PPA](https://launchpad.net/ubuntu/+ppas). LaunchPad then builds the binary `.deb` files for each target architecture.

## Packages

| Package      | Description                       | Architecture |
|--------------|-----------------------------------|--------------|
| `ank-server` | Ankaios server                    | amd64, arm64 |
| `ank-agent`  | Ankaios agent                     | amd64, arm64 |
| `ank`        | Ankaios CLI                       | amd64, arm64 |
| `ankaios`    | Meta-package (installs all three) | all          |

Installed paths:

- Binaries: `/usr/bin/`
- Config files: `/etc/ankaios/`
- Systemd services: `/lib/systemd/system/`

Version format: ```<upstream>~<series><ppa_revision>```.

## Automated Publishing

Publishing is handled by `.github/workflows/publish_deb.yml`, which can be triggered from the release workflow or manually via `workflow_dispatch`.

## Manual Usage

```bash
# Vendor dependencies
just vendor

# Create and sign the deb package
GPG_KEY_ID=<key-id> GPG_PASSPHRASE=<passphrase> bash tools/debian_utils/create_deb.sh
```

The script generates one signed source package per series. Output artifacts land in `dist/`:

```txt
dist/
  ankaios_<version>~noble1.dsc
  ankaios_<version>~noble1.tar.gz
  ankaios_<version>~noble1_source.buildinfo
  ankaios_<version>~noble1_source.changes
  ankaios_<version>~jammy1.dsc
  ...
```

### Stage 2 — Upload to LaunchPad

```bash
dput ppa:ankaios/ankaios dist/ankaios_<version>~<series><revision>_source.changes
```

## Local Testing

To test the build without uploading to LaunchPad, extract and build the binary packages locally:

```bash
# 1. Extract the source package
dpkg-source -x dist/ankaios_<version>.dsc /tmp/ankaios-test/

# 2. Build binary packages (-d skips dependency check; cargo/rustc installed via rustup are invisible to dpkg)
cd /tmp/ankaios-test
dpkg-buildpackage -b -us -uc -d

# 3. Install
sudo dpkg -i ../ank-server_<version>_amd64.deb \
              ../ank-agent_<version>_amd64.deb \
              ../ank_<version>_amd64.deb \
              ../ankaios_<version>_all.deb

# 4. Uninstall (--purge also removes config files from /etc/ankaios/)
sudo dpkg --purge ankaios ank-server ank-agent ank
```

### Linting

```bash
lintian /tmp/ank-server_<version>_amd64.deb
```

Known warnings:

- `embedded-library libyaml` — `serde_yaml` statically links `unsafe-libyaml` (a Rust port); unavoidable for Rust packages

## Useful Links

- [PPA - documentation.ubuntu.com](https://documentation.ubuntu.com/launchpad/user/reference/packaging/ppas/ppa/index.html)
- [Create deb Packages - youtube.com](https://www.youtube.com/watch?v=ep88vVfzDAo)
- [Repositories - help.ubuntu.com](https://help.ubuntu.com/community/Repositories/Ubuntu)
- [Debian New Maintainers' Guide - debian.org](https://www.debian.org/doc/manuals/maint-guide/)
- [Create source package - debian.org](https://www.debian.org/doc/debian-policy/ch-controlfields.html#debian-source-package-template-control-files-debian-control)
