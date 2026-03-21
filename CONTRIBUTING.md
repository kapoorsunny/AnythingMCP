# Contributing to mcpw

Thank you for your interest in contributing to mcpw! Here's how you can help.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/your-username/AnythingMCP.git`
3. Create a branch: `git checkout -b my-feature`
4. Install Rust: https://rustup.rs

## Development

```bash
cargo build              # Build
cargo test               # Run tests
cargo clippy -- -D warnings  # Lint
cargo fmt                # Format
```

## Making Changes

1. Write tests first (TDD)
2. Make your changes
3. Ensure all tests pass: `cargo test -- --test-threads=1`
4. Ensure clippy is clean: `cargo clippy -- -D warnings`
5. Format your code: `cargo fmt`
6. Commit with a clear message

## Pull Request Process

1. Update README.md if you've added new commands or changed behavior
2. Update help text in `src/cli.rs` for any CLI changes
3. Add tests for new functionality
4. Ensure CI passes
5. Request review

## Code Standards

- No `unwrap()` in production code — use `Result` and `?`
- No `unsafe` code
- No shell interpreters — use `Command::arg()` for subprocess arguments
- All public types should have doc comments
- Follow existing patterns for new commands/modules

## Reporting Bugs

Open an issue with:
- mcpw version (`mcpw --version`)
- Operating system
- Steps to reproduce
- Expected vs actual behavior

## Feature Requests

Open an issue describing:
- The problem you're trying to solve
- Your proposed solution
- Alternatives you've considered
