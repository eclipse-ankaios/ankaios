.DEFAULT_GOAL := build

.PHONY:build
release: check-env
	cargo build -v --release

build: check-env
	cargo build

test: check-env
	cargo install cargo-nextest --locked
	cargo nextest run

check-env:
	if [ ! -f /usr/bin/protoc ] ; \
	then \
	    echo "\nERROR: Protobuf compiler needs to be installed"; \
	    echo "       Debian/Ubuntu: install \"protobuf-compiler\"\n"; \
	    exit 1; \
	fi;
