default:
    @just --list

# Build the project in release mode
build:
    cargo build --release

# Run the project
run:
    cargo run

# Run all unit tests
test:
    cargo test

# Run Clippy checks
clippy:
    cargo clippy --all-targets -- -D warnings

# Clean build artifacts
clean:
    cargo clean

# Format the codebase
fmt:
    cargo fmt --all
