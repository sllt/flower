alias b := local

local:
    @echo "build on {{os()}}/{{arch()}}".
    @cargo build -p flower-bin --release

local-dev:
	cargo build -p flower-bin

mipsel:
	./misc/build_cross.sh mipsel-unknown-linux-musl

mips:
	./misc/build_cross.sh mips-unknown-linux-musl

test:
	cargo test -p flower -- --nocapture

# Force a re-generation of protobuf files.
proto-gen:
	touch flower/build.rs
	PROTO_GEN=1 cargo build -p flower

ios:
	cargo lipo --release -p flower-ffi
	cbindgen --config flower-ffi/cbindgen.toml flower-ffi/src/lib.rs > target/universal/release/flower.h

ios-dev:
	cargo lipo -p flower-ffi
	cbindgen --config flower-ffi/cbindgen.toml flower-ffi/src/lib.rs > target/universal/debug/flower.h

ios-opt:
	RUSTFLAGS="-Z strip=symbols" cargo lipo --release --targets aarch64-apple-ios --manifest-path flower-ffi/Cargo.toml --no-default-features --features "default-openssl"
	cbindgen --config flower-ffi/cbindgen.toml flower-ffi/src/lib.rs > target/universal/release/flower.h

lib:
	cargo build -p flower-ffi --release
	cbindgen --config flower-ffi/cbindgen.toml flower-ffi/src/lib.rs > target/release/flower.h

lib-dev:
	cargo build -p flower-ffi
	cbindgen --config flower-ffi/cbindgen.toml flower-ffi/src/lib.rs > target/debug/flower.h

android:
	cargo build -p flower-ffi --release --target aarch64-linux-android
	cbindgen --config flower-ffi/cbindgen.toml flower-ffi/src/lib.rs > target/aarch64-linux-android/release/flower.h
