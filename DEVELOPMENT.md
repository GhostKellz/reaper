# DEVELOPMENT.md

# Reaper Development & Error Handling Guide

This document covers common error types, async patterns, and best practices for robust code in the Reaper Zig codebase.

## Error Handling Patterns
- All fallible functions should return error unions `!T` or custom error types.
- Use `try` for error propagation instead of `.unwrap()` or panics.
- Add context to errors with custom error messages and stack traces.
- Example:
  ```zig
  const std = @import("std");
  const ConfigError = error{FileNotFound, ParseError};
  
  fn readConfig(allocator: std.mem.Allocator) ![]u8 {
      return std.fs.cwd().readFileAlloc(allocator, "/etc/reap/reap.toml", 1024 * 1024) catch |err| switch (err) {
          error.FileNotFound => {
              std.log.err("Config file not found: /etc/reap/reap.toml", .{});
              return ConfigError.FileNotFound;
          },
          else => return err,
      };
  }
  ```

## Async & Memory Management
- Use zsync for async operations - never block the async executor.
- All memory allocations must use the provided allocator, never use global allocator directly.
- Use `defer` for cleanup and `errdefer` for error cleanup paths.
- All install/upgrade flows are async using zsync runtime with zero-allocation fast paths.

## Common Errors & Fixes
- **OutOfMemory**: Use arena allocators for temporary allocations, defer cleanup
- **Memory leaks**: Always pair allocations with `defer allocator.free()` or use arena
- **Async blocking**: Use zsync async functions, never `std.time.sleep()` in async code
- **Network errors**: Use async HTTP with proper timeout and error handling
- **Test failures**: Use `testing.expect()` and proper error propagation

## Debugging Tips
- Use `std.debug.print()` for quick debugging
- Run `zig build test` and `zig build -Doptimize=Debug` regularly
- For async issues, ensure you're using zsync primitives correctly
- For memory issues, run with `zig build -Doptimize=Debug` and check for leaks

## Performance Guidelines
- Use `std.ArrayList` for dynamic arrays, pre-allocate when size is known
- Prefer stack allocation over heap when possible
- Use `@memcpy` and `@memset` for bulk operations
- Profile with `zig build -Doptimize=ReleaseFast -Dcpu=native`

## Adding New Features
- Follow the error handling and async patterns above
- Use proper memory management with allocators
- Document new error types or patterns here as needed
- For major changes, update this file with new best practices

---

For more, see ROADMAP.md and inline code comments.
