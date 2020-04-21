check:
	cargo check --workspace --all-features --tests --examples --benches

build:
	cargo build --workspace --all-features --tests --examples --benches

test:
	cargo test --workspace --all-features

clippy:
	cargo clippy --workspace --all-targets --all-features -- -D warnings $$(source ".clippy.args")

fmt:
	cargo +nightly fmt

fmt-check:
	cargo +nightly fmt -- --check

audit:
	cargo audit

release: release-bump build
	git commit -am "Bump to version $$(cargo read-manifest | jq .version)"
	git tag v$$(cargo read-manifest | jq -r .version)

release-bump:
	cargo bump

publish:
	git push && git push --tags


.PHONY:

