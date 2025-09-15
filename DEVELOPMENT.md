# DEVELOPMENT.md

# Reaper Development & Error Handling Guide

This document covers common error types, async/lifetime issues, and best practices for robust code in the Reaper codebase.

## Error Handling Patterns
- All fallible functions should return `Result<T, anyhow::Error>` or a custom `ReapError`.
- Use `?` for error propagation instead of `.unwrap()` or `.expect()`.
- Add context to errors with `anyhow::Context` for IO, process, and network operations.
- Example:
  ```rust
  use anyhow::{Result, Context};
  fn read_config() -> Result<String> {
      std::fs::read_to_string("/etc/reap/reap.toml")
          .context("Failed to read config file")
  }
  ```

## Async & Lifetime Issues
- Never hold references across `.await` in async functions. Use `Arc`, `Mutex`, or owned values.
- If you see errors like `E0597`, `E0308`, or `E0495`, check for references or temporaries across `.await`.
- Use `Arc<T>` for shared state in async code, and `Arc<Mutex<T>>` for mutable shared state.
- All install/upgrade flows are now async/parallel and use only native Rust logic (no yay/paru fallback).

## Common Errors & Fixes
- **unwrap() panics**: Replace with `?` and propagate errors.
- **Borrow checker/lifetime**: Use owned types or `Arc` for async/shared state.
- **Process/IO errors**: Always add `.context()` to error sources.
- **Network errors**: Use `anyhow` for error propagation and context.
- **Test failures**: Use `Result<(), anyhow::Error>` for all test functions, and propagate errors with `?`.

## Debugging Tips
- Use `dbg!()` or `println!()` for quick debugging.
- Run `cargo clippy` and `cargo test` regularly.
- For async issues, check for references or temporaries held across `.await`.
- For error handling, ensure all fallible code is covered by `Result` and `?`.

## Adding New Features
- Follow the error handling and async patterns above.
- Document new error types or patterns here as needed.
- For major changes, update this file with new best practices.

---

For more, see ROADMAP.md and inline code comments.
