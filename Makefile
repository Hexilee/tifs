build: 
	cargo build
release:
	cargo build --features "binc" --release
fmt:
	cargo fmt
mount: release
	RUST_LOG=info ./target/release/tifs $(config)
test:
	cargo test --all
lint:
	cargo clippy --all-targets -- -D warnings