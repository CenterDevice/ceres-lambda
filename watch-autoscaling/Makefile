all: check build test

DOCKER_BUILD_IMAGE = ekidd/rust-musl-builder:stable

todos:
	rg --vimgrep -g '!Makefile' -i todo 

check:
	cargo check

build:
	cargo build

test:
	cargo test

run:
	cargo run


.cargo.cache/git:
	docker run --rm -it -v "$$(pwd)":/home/rust/src -v "$$(pwd)/.cargo.cache/git":/home/rust/.cargo/git $(DOCKER_BUILD_IMAGE) sudo chown -R rust:rust /home/rust/.cargo/git

.cargo.cache/registry:
	docker run --rm -it -v "$$(pwd)":/home/rust/src -v "$$(pwd)/.cargo.cache/registry":/home/rust/.cargo/registry $(DOCKER_BUILD_IMAGE) sudo chown -R rust:rust /home/rust/.cargo/registry

cross_compile: target/x86_64-unknown-linux-musl/release/bootstrap
target/x86_64-unknown-linux-musl/release/bootstrap:	.cargo.cache/git .cargo.cache/registry
	docker run --rm -it -v "$$(pwd)":/home/rust/src -v "$$(pwd)/.cargo.cache/git":/home/rust/.cargo/git -v "$$(pwd)/.cargo.cache/registry":/home/rust/.cargo/registry $(DOCKER_BUILD_IMAGE) cargo build --release


upgrade: upgrade-docker-images

upgrade-docker-images:
	docker pull $(DOCKER_BUILD_IMAGE)


clean:
	cargo clean
	docker run --rm -it -v "$$(pwd)":/home/rust/src -v "$$(pwd)/.cargo.cache/git":/home/rust/.cargo/git -v "$$(pwd)/.cargo.cache/registry":/home/rust/.cargo/registry $(DOCKER_BUILD_IMAGE) cargo clean

