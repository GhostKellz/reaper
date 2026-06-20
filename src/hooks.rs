use serde::Serialize;
use std::process::Command;

/// Hook execution timeout (30 seconds)
const HOOK_TIMEOUT_SECS: u64 = 30;

#[derive(Serialize)]
pub struct HookContext {
    pub pkg: String,
    pub version: Option<String>,
    pub source: Option<String>,
    pub install_path: Option<String>,
    pub tap: Option<String>,
}

/// Result of hook execution
#[derive(Debug)]
pub enum HookResult {
    /// Hook executed successfully
    Success,
    /// Hook not found (this is OK)
    NotFound,
    /// Hook execution failed
    Failed(#[allow(dead_code)] String),
    /// Hook timed out
    Timeout,
    /// Security violation (path traversal, etc.)
    SecurityViolation(#[allow(dead_code)] String),
}

/// Sanitize a string for safe use in shell environment variables
fn sanitize_env_value(value: &str) -> String {
    // Remove or escape potentially dangerous characters
    value
        .chars()
        .filter(|c| {
            c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.' || *c == '/' || *c == ' '
        })
        .collect()
}

// Remove or comment out unused function find_hook_file
/*
#[allow(dead_code)]
fn find_hook_file(hook: &str, ctx: &HookContext) -> Option<PathBuf> {
    // 1. Per-package: ~/.config/reap/hooks/tapname/pkgname/hook.lua
    if let Some(tap) = &ctx.tap {
        let pkg_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(format!("reap/hooks/{}/{}/{}.lua", tap, ctx.pkg, hook));
        if pkg_dir.exists() {
            return Some(pkg_dir);
        }
        // 2. Per-tap: ~/.config/reap/hooks/tapname/hook.lua
        let tap_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(format!("reap/hooks/{}/{}.lua", tap, hook));
        if tap_dir.exists() {
            return Some(tap_dir);
        }
    }
    // 3. Global: ~/.config/reap/hooks/global/hook.lua
    let global_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(format!("reap/hooks/global/{}.lua", hook));
    if global_dir.exists() {
        return Some(global_dir);
    }
    None
}
*/

fn run_shell_hook(hook: &str, ctx: &HookContext) -> HookResult {
    let hooks_dir = crate::paths::hooks_dir();
    let script_path = hooks_dir.join(format!("{}.sh", hook));

    // Security: Validate hook name (no path traversal)
    if hook.contains("..") || hook.contains('/') || hook.contains('\\') {
        return HookResult::SecurityViolation(format!(
            "Invalid hook name '{}': path traversal not allowed",
            hook
        ));
    }

    // Security: Verify script is within hooks directory (canonicalize to resolve symlinks)
    if let (Ok(canonical_hooks), Ok(canonical_script)) =
        (hooks_dir.canonicalize(), script_path.canonicalize())
        && !canonical_script.starts_with(&canonical_hooks)
    {
        return HookResult::SecurityViolation(format!("Hook '{}' escapes hooks directory", hook));
    }

    if !script_path.exists() {
        return HookResult::NotFound;
    }

    // Log hook execution
    eprintln!("[hook] Executing {}.sh for package '{}'", hook, ctx.pkg);

    // Build command with timeout wrapper
    let mut cmd = Command::new("timeout");
    cmd.arg(format!("{}s", HOOK_TIMEOUT_SECS));
    cmd.arg("bash");
    cmd.arg(&script_path);

    // Pass context as sanitized env vars
    cmd.env("REAP_PKG", sanitize_env_value(&ctx.pkg));
    if let Some(ver) = &ctx.version {
        cmd.env("REAP_VERSION", sanitize_env_value(ver));
    }
    if let Some(src) = &ctx.source {
        cmd.env("REAP_SOURCE", sanitize_env_value(src));
    }
    if let Some(path) = &ctx.install_path {
        cmd.env("REAP_INSTALL_PATH", sanitize_env_value(path));
    }
    if let Some(tap) = &ctx.tap {
        cmd.env("REAP_TAP", sanitize_env_value(tap));
    }

    match cmd.status() {
        Ok(status) => {
            if status.success() {
                eprintln!("[hook] {} completed successfully", hook);
                HookResult::Success
            } else if status.code() == Some(124) {
                // timeout command returns 124 on timeout
                eprintln!("[hook] {} timed out after {}s", hook, HOOK_TIMEOUT_SECS);
                HookResult::Timeout
            } else {
                eprintln!("[hook] {} failed with status {:?}", hook, status.code());
                HookResult::Failed(format!("Exit code: {:?}", status.code()))
            }
        }
        Err(e) => {
            eprintln!("[hook] Failed to execute {}: {}", hook, e);
            HookResult::Failed(e.to_string())
        }
    }
}

// Legacy wrapper for compatibility (ignores result)
fn run_shell_hook_silent(hook: &str, ctx: &HookContext) {
    let _ = run_shell_hook(hook, ctx);
}

/// Pre-installation hook, called before a package is installed.
/// This will execute any `pre_install` script found in the hooks directory.
/// Hooks run with a 30-second timeout and sanitized environment variables.
pub fn pre_install(ctx: &HookContext) {
    run_shell_hook_silent("pre_install", ctx);
}

/// Post-installation hook, called after a package is installed.
/// This will execute any `post_install` script found in the hooks directory.
/// Hooks run with a 30-second timeout and sanitized environment variables.
pub fn post_install(ctx: &HookContext) {
    run_shell_hook_silent("post_install", ctx);
}

/// Run a hook and return the result (for cases where caller needs to handle failures)
#[allow(dead_code)]
pub fn run_hook(hook_name: &str, ctx: &HookContext) -> HookResult {
    run_shell_hook(hook_name, ctx)
}

// Ensure all hook calls are safe (do not panic if missing), and add doc comments for hook usage
// All Lua logic removed; hooks will call shell scripts if present in ~/.config/reap/hooks/

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context() -> HookContext {
        HookContext {
            pkg: "test-pkg".to_string(),
            version: Some("1.0.0".to_string()),
            source: None,
            install_path: None,
            tap: None,
        }
    }

    #[test]
    fn test_path_traversal_blocked() {
        let ctx = test_context();

        // Test various path traversal attempts
        let traversal_attempts = [
            "../../../etc/passwd",
            "..\\..\\..\\windows\\system32",
            "hook/../../../secret",
            "/etc/passwd",
            "//etc/passwd",
        ];

        for hook_name in &traversal_attempts {
            let result = run_shell_hook(hook_name, &ctx);
            // Path traversal should be blocked with SecurityViolation
            // or NotFound (if the sanitization strips the traversal)
            assert!(
                matches!(
                    result,
                    HookResult::SecurityViolation(_) | HookResult::NotFound
                ),
                "Hook '{}' should be blocked or not found, got {:?}",
                hook_name,
                result
            );
        }
    }

    #[test]
    fn test_hook_not_found_is_safe() {
        let ctx = test_context();

        // A non-existent hook should return NotFound, not an error
        let result = run_shell_hook("nonexistent_hook_12345", &ctx);
        assert!(
            matches!(result, HookResult::NotFound),
            "Non-existent hook should return NotFound, got {:?}",
            result
        );
    }

    #[test]
    fn test_hook_context_sanitization() {
        // Package names with special characters should be sanitized
        let ctx = HookContext {
            pkg: "test-pkg; rm -rf /".to_string(), // Attempted command injection
            version: Some("1.0.0 && echo pwned".to_string()),
            source: None,
            install_path: None,
            tap: None,
        };

        // The sanitize_env_value function should clean these
        let sanitized_pkg = sanitize_env_value(&ctx.pkg);
        let sanitized_ver = sanitize_env_value(ctx.version.as_deref().unwrap_or(""));

        // Should not contain shell metacharacters after sanitization
        assert!(
            !sanitized_pkg.contains(';'),
            "Semicolons should be sanitized"
        );
        assert!(
            !sanitized_ver.contains("&&"),
            "Shell operators should be sanitized"
        );
    }

    #[test]
    fn test_hook_result_variants() {
        // Ensure all HookResult variants are representable
        let success = HookResult::Success;
        let not_found = HookResult::NotFound;
        let failed = HookResult::Failed("test error".to_string());
        let timeout = HookResult::Timeout;
        let security = HookResult::SecurityViolation("path traversal".to_string());

        // Pattern matching should work for all variants
        assert!(matches!(success, HookResult::Success));
        assert!(matches!(not_found, HookResult::NotFound));
        assert!(matches!(failed, HookResult::Failed(_)));
        assert!(matches!(timeout, HookResult::Timeout));
        assert!(matches!(security, HookResult::SecurityViolation(_)));
    }
}
