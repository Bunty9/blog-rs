default: test

build:
    cargo build --workspace

test:
    cargo test --workspace

fmt:
    cargo fmt --all

lint:
    cargo clippy --workspace --all-targets -- -D warnings

snap-review:
    cargo insta review

coverage:
    cargo llvm-cov --workspace --fail-under-lines 70
