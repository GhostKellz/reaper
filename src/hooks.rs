use serde::Serialize;
use std::process::Command;

#[derive(Serialize)]
pub struct HookContext {
    pub pkg: String,
    pub version: Option<String>,
    pub source: Option<String>,
    pub install_path: Option<String>,
    pub tap: Option<String>,
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

fn run_shell_hook(hook: &str, ctx: &HookContext) {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    let script_path = home.join(format!(".config/reap/hooks/{}.sh", hook));
    if script_path.exists() {
        let mut cmd = Command::new("bash");
        cmd.arg(&script_path);
        // Pass context as env vars
        cmd.env("REAP_PKG", &ctx.pkg);
        if let Some(ver) = &ctx.version {
            cmd.env("REAP_VERSION", ver);
        }
        if let Some(src) = &ctx.source {
            cmd.env("REAP_SOURCE", src);
        }
        if let Some(path) = &ctx.install_path {
            cmd.env("REAP_INSTALL_PATH", path);
        }
        if let Some(tap) = &ctx.tap {
            cmd.env("REAP_TAP", tap);
        }
        let _ = cmd.status();
    }
}

fn run_lua_hook(_hook: &str, _ctx: &HookContext) {
    // Lua support is stubbed for future advanced setup.
}

/// Pre-installation hook, called before a package is installed.
/// This will execute any `pre_install` script found in the hooks directory.
pub fn pre_install(ctx: &HookContext) {
    run_shell_hook("pre_install", ctx);
    run_lua_hook("pre_install", ctx);
}

/// Post-installation hook, called after a package is installed.
/// This will execute any `post_install` script found in the hooks directory.
pub fn post_install(ctx: &HookContext) {
    run_shell_hook("post_install", ctx);
    run_lua_hook("post_install", ctx);
}

// Ensure all hook calls are safe (do not panic if missing), and add doc comments for hook usage
// All Lua logic removed; hooks will call shell scripts if present in ~/.config/reap/hooks/

// No async/parallel flows in hooks.rs; nothing to change for prompt 2
