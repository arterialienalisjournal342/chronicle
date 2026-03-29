use anyhow::{Context as _, Result};
use std::fs;
use std::io::{self, BufRead as _, IsTerminal as _, Write as _};

use crate::canon::levels::L3_WARNING;
use crate::config::{self, CliOverrides};
use crate::git;

// ---------------------------------------------------------------------------
// chronicle init
// ---------------------------------------------------------------------------

/// Handle `chronicle init [--remote <url>]`.
///
/// Creates the config file (if absent), generates a machine name (if none),
/// initializes the local git repo, and prints a confirmation.  Safe to run
/// more than once — existing config and repo state are preserved.
pub fn handle_init(remote: Option<String>) -> Result<()> {
    let config_path = config::default_config_path();
    let config_existed = config_path.exists();

    // Load existing config, or start from built-in defaults.
    let mut cfg = config::load(Some(&config_path), &CliOverrides::default())
        .context("failed to load configuration")?;

    // Generate machine name if not already set.
    if cfg.general.machine_name.is_empty() {
        cfg.general.machine_name = config::machine_name::generate();
    }

    // Apply --remote flag (highest CLI precedence).
    if let Some(url) = remote {
        cfg.storage.remote_url = url;
    }

    // Prompt for remote URL if still unset and stdin is a TTY.
    if cfg.storage.remote_url.is_empty() && io::stdin().is_terminal() {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        write!(out, "Remote git URL (leave blank to skip): ")?;
        out.flush()?;

        let stdin = io::stdin();
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        let url = line.trim().to_owned();
        if !url.is_empty() {
            cfg.storage.remote_url = url;
        }
    }

    // Warn if L3 freeform canonicalization is active.
    if cfg.canonicalization.level >= 3 {
        eprintln!("{L3_WARNING}");
    }

    // Write config file (create parent directories if needed).
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory {}", parent.display()))?;
    }
    let toml_content =
        toml::to_string_pretty(&cfg).context("failed to serialize configuration to TOML")?;
    fs::write(&config_path, &toml_content)
        .with_context(|| format!("failed to write config file {}", config_path.display()))?;

    // Initialize (or open) the git repository.
    let repo_path = config::expand_path(&cfg.storage.repo_path);
    let remote_url = if cfg.storage.remote_url.is_empty() {
        None
    } else {
        Some(cfg.storage.remote_url.as_str())
    };

    let manager = git::RepoManager::init_or_open(&repo_path, remote_url)
        .context("failed to initialize git repository")?;
    manager
        .ensure_working_tree()
        .context("failed to set up repository working tree")?;
    manager
        .ensure_manifest()
        .context("failed to initialize repository manifest")?;

    // Print confirmation.
    println!("✓ Chronicle initialized");
    println!("  Machine name : {}", cfg.general.machine_name);
    println!("  Config file  : {}", config_path.display());
    println!("  Repository   : {}", repo_path.display());
    if !cfg.storage.remote_url.is_empty() {
        println!("  Remote URL   : {}", cfg.storage.remote_url);
    }
    if config_existed {
        println!("\nNote: existing config preserved (no values overwritten).");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// chronicle import
// ---------------------------------------------------------------------------

/// Handle `chronicle import [--agent <pi|claude|all>] [--dry-run]`.
pub fn handle_import(_agent: String, _dry_run: bool) -> Result<()> {
    println!("not implemented: import");
    Ok(())
}

// ---------------------------------------------------------------------------
// chronicle sync
// ---------------------------------------------------------------------------

/// Handle `chronicle sync [--dry-run] [--quiet]`.
pub fn handle_sync(_dry_run: bool, _quiet: bool) -> Result<()> {
    println!("not implemented: sync");
    Ok(())
}

// ---------------------------------------------------------------------------
// chronicle push
// ---------------------------------------------------------------------------

/// Handle `chronicle push [--dry-run]`.
pub fn handle_push(_dry_run: bool) -> Result<()> {
    println!("not implemented: push");
    Ok(())
}

// ---------------------------------------------------------------------------
// chronicle pull
// ---------------------------------------------------------------------------

/// Handle `chronicle pull [--dry-run]`.
pub fn handle_pull(_dry_run: bool) -> Result<()> {
    println!("not implemented: pull");
    Ok(())
}

// ---------------------------------------------------------------------------
// chronicle status
// ---------------------------------------------------------------------------

/// Handle `chronicle status`.
pub fn handle_status() -> Result<()> {
    println!("not implemented: status");
    Ok(())
}

// ---------------------------------------------------------------------------
// chronicle errors
// ---------------------------------------------------------------------------

/// Handle `chronicle errors [--limit <n>]`.
pub fn handle_errors(_limit: Option<usize>) -> Result<()> {
    println!("not implemented: errors");
    Ok(())
}

// ---------------------------------------------------------------------------
// chronicle config
// ---------------------------------------------------------------------------

/// Handle `chronicle config [<key>] [<value>]`.
pub fn handle_config(_key: Option<String>, _value: Option<String>) -> Result<()> {
    println!("not implemented: config");
    Ok(())
}

// ---------------------------------------------------------------------------
// chronicle schedule *
// ---------------------------------------------------------------------------

/// Handle `chronicle schedule install`.
pub fn handle_schedule_install() -> Result<()> {
    println!("not implemented: schedule install");
    Ok(())
}

/// Handle `chronicle schedule uninstall`.
pub fn handle_schedule_uninstall() -> Result<()> {
    println!("not implemented: schedule uninstall");
    Ok(())
}

/// Handle `chronicle schedule status`.
pub fn handle_schedule_status() -> Result<()> {
    println!("not implemented: schedule status");
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Core init logic extracted for testability (avoids touching real home dir).
    fn init_with_config_path(config_path: &std::path::Path, remote: Option<String>) -> Result<()> {
        let config_existed = config_path.exists();

        let mut cfg = config::load(Some(config_path), &CliOverrides::default())
            .context("failed to load configuration")?;

        if cfg.general.machine_name.is_empty() {
            cfg.general.machine_name = config::machine_name::generate();
        }

        if let Some(url) = remote {
            cfg.storage.remote_url = url;
        }

        let toml_content =
            toml::to_string_pretty(&cfg).context("failed to serialize configuration to TOML")?;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(config_path, &toml_content)?;

        let repo_path = config::expand_path(&cfg.storage.repo_path);
        let remote_url = if cfg.storage.remote_url.is_empty() {
            None
        } else {
            Some(cfg.storage.remote_url.as_str())
        };

        let manager = git::RepoManager::init_or_open(&repo_path, remote_url)
            .context("failed to initialize git repository")?;
        manager.ensure_working_tree()?;
        manager.ensure_manifest()?;

        if config_existed {
            // idempotent — just confirm without printing anything in tests
        }

        Ok(())
    }

    // -----------------------------------------------------------------------

    #[test]
    fn init_creates_config_file() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("chronicle").join("config.toml");
        let repo_path = dir.path().join("repo");

        // Start with no config.
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        let toml = format!("[storage]\nrepo_path = \"{}\"\n", repo_path.display());
        std::fs::write(&config_path, &toml).unwrap();

        init_with_config_path(&config_path, None).unwrap();

        assert!(config_path.exists(), "config file should exist after init");
    }

    #[test]
    fn init_generates_machine_name() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("chronicle").join("config.toml");
        let repo_path = dir.path().join("repo");

        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        let toml = format!("[storage]\nrepo_path = \"{}\"\n", repo_path.display());
        std::fs::write(&config_path, &toml).unwrap();

        init_with_config_path(&config_path, None).unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        let cfg: crate::config::schema::Config = toml::from_str(&content).unwrap();
        assert!(
            !cfg.general.machine_name.is_empty(),
            "machine name should be generated"
        );
        assert!(
            cfg.general.machine_name.contains('-'),
            "machine name should be adjective-animal format"
        );
    }

    #[test]
    fn init_preserves_existing_machine_name() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("chronicle").join("config.toml");
        let repo_path = dir.path().join("repo");

        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        let toml = format!(
            "[general]\nmachine_name = \"happy-hippo\"\n\n[storage]\nrepo_path = \"{}\"\n",
            repo_path.display()
        );
        std::fs::write(&config_path, &toml).unwrap();

        init_with_config_path(&config_path, None).unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        let cfg: crate::config::schema::Config = toml::from_str(&content).unwrap();
        assert_eq!(
            cfg.general.machine_name, "happy-hippo",
            "existing machine name should be preserved"
        );
    }

    #[test]
    fn init_sets_remote_from_flag() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("chronicle").join("config.toml");
        let repo_path = dir.path().join("repo");

        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        let toml = format!("[storage]\nrepo_path = \"{}\"\n", repo_path.display());
        std::fs::write(&config_path, &toml).unwrap();

        init_with_config_path(
            &config_path,
            Some("git@example.com:user/sessions.git".to_owned()),
        )
        .unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        let cfg: crate::config::schema::Config = toml::from_str(&content).unwrap();
        assert_eq!(
            cfg.storage.remote_url, "git@example.com:user/sessions.git",
            "remote URL should be written to config"
        );
    }

    #[test]
    fn init_initializes_git_repo() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("chronicle").join("config.toml");
        let repo_path = dir.path().join("repo");

        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        let toml = format!("[storage]\nrepo_path = \"{}\"\n", repo_path.display());
        std::fs::write(&config_path, &toml).unwrap();

        init_with_config_path(&config_path, None).unwrap();

        // Git repo should exist with expected structure.
        assert!(
            repo_path.join(".git").exists() || repo_path.join("HEAD").exists(),
            "git repo should exist at repo_path"
        );
        assert!(
            repo_path.join("pi").join("sessions").exists(),
            "pi/sessions/ directory should exist"
        );
        assert!(
            repo_path.join("claude").join("projects").exists(),
            "claude/projects/ directory should exist"
        );
        assert!(
            repo_path.join(".chronicle").exists(),
            ".chronicle/ directory should exist"
        );
    }

    #[test]
    fn init_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("chronicle").join("config.toml");
        let repo_path = dir.path().join("repo");

        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        let toml = format!(
            "[general]\nmachine_name = \"bold-badger\"\n\n[storage]\nrepo_path = \"{}\"\n",
            repo_path.display()
        );
        std::fs::write(&config_path, &toml).unwrap();

        // Run twice — second call must succeed without error.
        init_with_config_path(&config_path, None).unwrap();
        init_with_config_path(&config_path, None).unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        let cfg: crate::config::schema::Config = toml::from_str(&content).unwrap();
        assert_eq!(
            cfg.general.machine_name, "bold-badger",
            "machine name must remain stable across repeated init calls"
        );
    }

    #[test]
    fn init_manifest_exists_after_init() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("chronicle").join("config.toml");
        let repo_path = dir.path().join("repo");

        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        let toml = format!("[storage]\nrepo_path = \"{}\"\n", repo_path.display());
        std::fs::write(&config_path, &toml).unwrap();

        init_with_config_path(&config_path, None).unwrap();

        let manifest_path = repo_path.join(".chronicle").join("manifest.json");
        assert!(
            manifest_path.exists(),
            "manifest.json should exist after init"
        );
    }

    #[test]
    fn init_writes_config_with_correct_toml() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("chronicle").join("config.toml");
        let repo_path = dir.path().join("repo");

        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        let toml = format!("[storage]\nrepo_path = \"{}\"\n", repo_path.display());
        std::fs::write(&config_path, &toml).unwrap();

        init_with_config_path(&config_path, None).unwrap();

        // Must be valid TOML that round-trips.
        let content = std::fs::read_to_string(&config_path).unwrap();
        let result: Result<crate::config::schema::Config, _> = toml::from_str(&content);
        assert!(result.is_ok(), "written config must be valid TOML");
    }
}
