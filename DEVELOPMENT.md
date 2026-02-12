# Development Guide

This guide provides information about the development tools and workflows available for this project.

## Quick Start

### Project Setup

This project uses [DevContainers](https://containers.dev/) for a consistent development environment. The devcontainer includes the rust toolchain, the necessary tooling and the required VSCode extensions.

## Commands and Scripts

The project provides [just](https://github.com/casey/just) commands for common development workflows. For a complete list of available commands, check the [justfile](justfile) or directly run `just --list`.

In addition, the [tools](tools/) folder contains helper scripts for specific tasks. Check the tools [readme](tools/README.md) for more info.

## Additional Resources

- [Contributing Guidelines](CONTRIBUTING.md)
- [Rust Coding Guidelines](https://eclipse-ankaios.github.io/ankaios/main/development/rust-coding-guidelines/)
- [Unit Verification Strategy](https://eclipse-ankaios.github.io/ankaios/main/development/unit-verification/)
