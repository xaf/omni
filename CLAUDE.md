# Omni Codebase Guide

## Build/Test/Lint Commands
- Build: `cargo build`
- Run tests: `cargo test` or `cargo test <test_name>` for specific test
- Run a specific bats test: `bats tests/test_<name>.bats`
- Lint code: `cargo clippy` or `omni lint`
- Format code: `cargo fmt` or `omni fix`
- Run with debug: `RUST_LOG=debug cargo run -- <command>`

## Code Style Guidelines
- Follow Rust idioms and the Rust API guidelines
- Use meaningful variable/function names in snake_case
- Modules: snake_case, Structs/Enums: PascalCase
- Use type annotations for function signatures
- Handle errors with Result/Option types, not unwrap()
- Use thiserror for custom error types
- Document public APIs with /// comments
- Organize imports with std first, then external crates, then internal modules
- Implement meaningful Debug/Display impls for user-facing types
- Use proper functional patterns (map/filter) over imperative loops when appropriate

## Code Organization
- Commands, utilities, and core functionality are organized in modules
- Tests are in .rs files or separate .bats files for integration tests
