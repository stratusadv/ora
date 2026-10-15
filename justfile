set windows-shell := ["cmd", "/c"]

clear := if os() == "windows" { "cls" } else { "clear" }

manifest := "src-tauri/Cargo.toml"

default:
    @just --list

install:
    bun install

dev:
    {{clear}}
    bun run tauri dev

build:
    {{clear}}
    bun run tauri build

check:
    {{clear}}
    cargo check --manifest-path {{manifest}} --all-targets

clippy:
    {{clear}}
    cargo clippy --manifest-path {{manifest}} --all-targets -- -D warnings

clippy-windows:
    {{clear}}
    cargo xwin clippy --manifest-path {{manifest}} --target x86_64-pc-windows-msvc --all-targets -- -D warnings

typecheck:
    {{clear}}
    bun run typecheck

lint:
    {{clear}}
    bun run lint

lint-fix:
    {{clear}}
    bun run lint:fix

tigerstyle:
    {{clear}}
    tigerstyle check --ignore TS004

verify:
    {{clear}}
    cargo +nightly fmt --manifest-path {{manifest}} -- --check
    cargo clippy --manifest-path {{manifest}} --all-targets -- -D warnings
    cargo test --manifest-path {{manifest}}
    tigerstyle check --ignore TS004
    bun run typecheck
    bun run lint
    bun run test

fmt:
    {{clear}}
    cargo +nightly fmt --manifest-path {{manifest}}

fmt-check:
    {{clear}}
    cargo +nightly fmt --manifest-path {{manifest}} -- --check

test:
    {{clear}}
    cargo test --manifest-path {{manifest}}
    bun run test

[linux]
e2e:
    {{clear}}
    ORA_API_HOST= ORA_API_KEY_LLM= ORA_API_KEY_TRANSCRIPTION= CARGO_TARGET_DIR={{justfile_directory()}}/src-tauri/target/e2e bun run tauri build --debug --no-bundle
    dbus-run-session -- xvfb-run -a bun run test:e2e

clean:
    {{clear}}
    cargo clean --manifest-path {{manifest}}
