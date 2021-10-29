build: 
	cargo build
release:
	cargo build --release
install: release
	sh ./install.sh
fmt:
	cargo fmt
mount: release
	RUST_LOG=info target/release/tifs -m $(MOUNT_POINT)
test:
	cargo test --all
lint:
	cargo clippy --all-targets -- -D warnings