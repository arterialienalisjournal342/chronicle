---
date_created: 2026-04-03
date_modified: 2026-04-03
status: draft
audience: both
cross_references:
  - docs/001-architecture.md
  - docs/specs/001-initial-delivery.md
  - docs/specs/002-status-improvements.md
  - docs/specs/004-doctor-command.md
  - src/canon/mod.rs
  - src/canon/levels.rs
  - src/agents/mod.rs
  - src/git/mod.rs
  - src/scheduler/cron.rs
---

# Spec 005 — Windows Support

## 1. Goal

Bring chronicle to native Windows (PowerShell / cmd.exe) with full feature
parity to macOS and Linux:

- `init`, `import`, `sync`, `push`, `pull`, `status`, `errors`, `config`,
  `schedule`, and `doctor` all work on Windows.
- A Windows machine can sync session files with macOS and Linux machines
  through the same Git remote, with transparent path translation.
- Release artefacts: `x86_64-pc-windows-msvc` `.exe` on GitHub Releases,
  plus MSI installer and Scoop/winget packages.
- CI runs the full test suite on `windows-latest`.

## 2. Core Design Decisions

### 2.1 Path Representation in the Git Repository

All paths stored in the Git repository (canonicalized JSONL content and
directory names) use **forward slashes and no drive letter**.

```
C:\Users\brad\Dev\foo  →  {{SYNC_HOME}}/Dev/foo   (stored in Git)
/Users/bradmatic/Dev/foo  →  {{SYNC_HOME}}/Dev/foo  (stored in Git)
```

This gives the Git repo a single canonical form that every OS can produce and
consume.

### 2.2 De-canonicalization: Native Paths Per Machine

When de-canonicalizing, `{{SYNC_HOME}}` is replaced with the **local machine's
native home path**, including OS-appropriate separators:

| Machine OS | Stored token | De-canonicalized result |
|------------|-------------|------------------------|
| macOS | `{{SYNC_HOME}}/Dev/foo` | `/Users/bradmatic/Dev/foo` |
| Linux | `{{SYNC_HOME}}/Dev/foo` | `/home/brad/Dev/foo` |
| Windows | `{{SYNC_HOME}}/Dev/foo` | `C:\Users\brad\Dev\foo` |

Chronicle already stores the local home in `TokenRegistry`.  The Windows
extension adds: when the local home contains a drive letter and backslashes,
de-canonicalization converts forward slashes back to backslashes and prepends
the drive letter.

### 2.3 L1 Directory Name Encoding on Windows

Pi and Claude encode session directory names from the project path.
The drive letter is **not stripped** — it is encoded as a path component.
This is confirmed from Pi's source:

```javascript
// Pi session-manager.js
const safePath = `--${cwd.replace(/^[\/\\]/, "").replace(/[\/\\:]/g, "-")}--`;
```

The regex strips a single leading `/` or `\` (so Unix `/Users/…` loses its
leading slash), then replaces **each** `/`, `\`, and `:` with a single `-`.
Because a Windows path starts with a drive letter (`C:`), nothing is stripped;
both `:` and the following `\` each become their own `-`, producing a
double-dash between the drive letter and the first path component:

```
C:\Users\brad\Dev\foo
→ strip leading / or \: no change (C is not a slash)
→ replace each /, \, : with -: C--Users-brad-Dev-foo
                                  ^^ colon + backslash = two dashes
→ Pi format:    --C--Users-brad-Dev-foo--
→ Claude format:  C--Users-brad-Dev-foo   (no leading - on Windows paths;
                                             see note on leading-dash below)
```

This is **confirmed** from Pi's source and a live Windows Claude Code install.

For Unix paths the leading `/` is stripped, producing the same shape as today:

```
/Users/bradmatic/Dev/foo
→ strip leading /:        Users/bradmatic/Dev/foo
→ replace / with -:       Users-bradmatic-Dev-foo
→ Pi format:    --Users-bradmatic-Dev-foo--
→ Claude format: -Users-bradmatic-Dev-foo
```

After L1 canonicalization both forms collapse to `--{{SYNC_HOME}}-Dev-foo--`
(or `-{{SYNC_HOME}}-Dev-foo`) because the encoded home prefix (`C--Users-brad`
or `Users-bradmatic`) is matched and replaced by the token.

**Consequence:** Two projects on different drives on the same Windows machine
produce distinct encoded names (`--C--Users-brad-…--` vs `--D--Users-brad-…--`),
so there is no collision concern.

**Leading `-` prefix on Claude Windows paths:** On macOS/Linux the leading `-`
comes from prepending `-` after stripping the leading `/`.  On Windows there
is no leading `/` to strip; the observed directory name is `C--Users-brad`
(no leading `-`).  This needs to be accounted for in `ClaudeAgent::encode_dir`
and `ClaudeAgent::decode_dir` on Windows: do not prepend `-` when the path
does not start with `/`.

### 2.4 L2/L3 Canonicalization: Backslash Paths

On a Windows machine, session files may contain paths with backslashes.
The canonicalization engine must handle both:

- Native backslash paths: `C:\Users\brad\Dev\foo`
- Forward-slash paths (e.g., from tool output): `/c/Users/brad/Dev/foo`

**Approach:**

1. Normalize the input string before matching: replace `\` with `/` and strip
   any leading drive letter prefix (`C:/` → `/`).
2. Apply the existing `replace_in_text` / `try_canonicalize_path` logic on
   the normalized form.
3. The stored (canonicalized) form always uses forward slashes.
4. De-canonicalization converts the stored form back to native backslash paths
   on Windows.

This normalization is applied inside `TokenRegistry` when
`cfg!(target_os = "windows")` (or when a Windows home path is detected at
runtime — see §6.1).

### 2.5 Locking: Named Mutex on Windows

The Unix advisory `flock` is replaced on Windows with a **named mutex** via
the Windows API:

- Mutex name: `Global\chronicle-<sha256-of-repo-path>` (the hash makes it
  unique per repo, not machine-wide).
- Acquiring: `CreateMutexW` + `WaitForSingleObject(handle, 0)` (non-blocking).
- If the mutex is already held, the same staleness logic applies: check the
  lock file's PID and timestamp, break if stale.
- Releasing: `ReleaseMutex` + `CloseHandle` in a `Drop` impl.

The `windows-sys` crate (already in the Rust standard library's indirect
dependency tree via `std`) is used to avoid adding a heavyweight `windows-rs`
dependency.

### 2.6 Scheduler: Windows Task Scheduler

`chronicle schedule install/uninstall/status` on Windows uses
`schtasks.exe`:

| Unix command | Windows equivalent |
|----|---|
| `crontab -l` | `schtasks /Query /TN chronicle /FO CSV /NH` |
| `crontab -` (write) | `schtasks /Create /TN chronicle /TR "<exe> sync" /SC MINUTE /MO <interval>` |
| Delete entry | `schtasks /Delete /TN chronicle /F` |

The task is created under the current user's context (`/RU %USERNAME%`).
Interval mapping from `general.schedule_interval` to `/SC MINUTE /MO N`
follows the same logic as the existing cron expression builder.

A `scheduler::windows` module is added alongside `scheduler::cron`, sharing
the same `Scheduler` trait (extracted from the current cron implementation).

### 2.7 Config Path on Windows

XDG is not available on Windows.  The config file path becomes:

```
%APPDATA%\chronicle\config.toml
```

Resolved via `dirs::config_dir()` from the `dirs` crate (already a transitive
dependency; may need to be made direct).

## 3. Agent Session Paths on Windows

| Agent | Default sessions path (Windows) | Source |
|-------|-------------------------------|--------|
| Pi | `%USERPROFILE%\.pi\agent\sessions` | Confirmed: Pi uses `join(homedir(), ".pi", "agent", "sessions")` |
| Claude Code | `%USERPROFILE%\.claude\projects` | Confirmed: observed on Windows install |

Pi's path is **confirmed** from `config.js`:
```javascript
export function getSessionsDir() {
    return join(getAgentDir(), "sessions");   // ~/.pi/agent/sessions
}
```

Both paths are confirmed.  The research spike (§8) still needs to verify
the path separator used inside session JSONL field values on Windows
(backslash vs forward slash in `cwd`, `path`, etc.) before the L2/L3
canonicalization changes are finalized.

## 4. Release Artefacts

### 4.1 Binary

Target triple: `x86_64-pc-windows-msvc`.  Built in `release.yml` using
GitHub Actions `windows-latest` runner (MSVC toolchain).

The `.exe` is uploaded to GitHub Releases alongside the existing macOS and
Linux artefacts.

`aarch64-pc-windows-msvc` is deferred — ARM Windows usage is niche and
cross-compilation from CI is complex.

### 4.2 MSI Installer

Built with [WiX Toolset v4](https://wixtoolset.org/) (via `cargo-wix`).
The MSI:
- Installs `chronicle.exe` to `%ProgramFiles%\chronicle\`.
- Adds `%ProgramFiles%\chronicle\` to the user's `PATH`.
- Provides an uninstaller.

`cargo-wix` generates the WiX source from `Cargo.toml` metadata; a
`wix/main.wxs` template is committed for customization.

### 4.3 Scoop / winget

**Scoop:** A `chronicle.json` manifest is submitted to
[scoop-extras](https://github.com/ScoopInstaller/Extras) or a dedicated
`scoop-geekmuse` bucket.

**winget:** A YAML package manifest is submitted to the
[winget-pkgs](https://github.com/microsoft/winget-pkgs) community repository.

Both use the GitHub Releases `.exe` URL with its SHA256 hash.
These submissions happen after the first Windows release tag; they are not
blocking for the initial implementation.

## 5. CI Changes

### 5.1 `ci.yml`

Add a `windows-latest` matrix entry to the existing build/lint/test job:

```yaml
strategy:
  matrix:
    os: [ubuntu-latest, macos-latest, windows-latest]
```

All four quality checks run on Windows:
- `cargo test`
- `cargo clippy -- -D warnings`
- `cargo fmt --check`
- `cargo deny check`

`cargo deny` licence checks are OS-agnostic; no change needed.

### 5.2 `release.yml`

Add a `windows-latest` release job that:
1. Builds `x86_64-pc-windows-msvc` with `--release`.
2. Uploads `chronicle.exe` to the GitHub Release.
3. (Stretch) Runs `cargo wix` to build and upload the MSI.

## 6. Implementation Plan

### Phase 1 — Research Spike (partial)

Pi's session path and encoding are **confirmed** from source code.  The
remaining unknowns for `docs/research/004-windows-agent-paths.md`:

- Confirm Claude Code's actual session directory on a Windows install
  (expected: `%USERPROFILE%\\.claude\\projects`).
- Verify path separator used in `cwd` and `path` fields of Claude session
  files on Windows (backslash vs forward slash).
- Confirm `git2` / libgit2 handles `C:\\`-style repo paths on MSVC.
- Confirm `cargo deny` runs cleanly on `windows-latest`.

### Phase 2 — Core Path Handling

#### 2a. `src/agents/mod.rs` — `encode_dir` for Windows

Both `PiAgent::encode_dir` and `ClaudeAgent::encode_dir` currently only trim
leading `/` and only replace `/` (and `.` for Claude) with `-`.  On Windows
they must also handle `\` and `:`.

Pi — match the Pi source regex exactly:
```rust
// Before:
let inner = path.to_string_lossy()
    .trim_start_matches('/')
    .replace('/', "-");

// After:
let inner = path.to_string_lossy()
    .trim_start_matches(|c| c == '/' || c == '\\')
    .replace(['/', '\\', ':'], "-");
// C:\Users\brad\Dev\foo  →  C--Users-brad-Dev-foo  →  --C--Users-brad-Dev-foo--
```

Claude — same, plus retain the `.` → `-` replacement, and conditionally
omit the leading `-` prefix when the original path has no leading slash
(i.e., Windows drive-letter paths):
```rust
// After:
let s = path.to_string_lossy();
let has_leading_slash = s.starts_with('/') || s.starts_with('\\');
let inner = s
    .trim_start_matches(|c| c == '/' || c == '\\')
    .replace(['/', '\\', ':', '.'], "-");
if has_leading_slash {
    format!("-{inner}")                      // Unix: -Users-bradmatic-Dev-foo
} else {
    inner.to_string()                        // Windows: C--Users-brad-Dev-foo
}
// Confirmed: C:\Users\brad  →  C--Users-brad  (no leading dash)
```

`ClaudeAgent::decode_dir` must also be updated to accept names that do **not**
start with `-` (i.e., Windows-encoded names starting with a drive letter).

#### 2b. `src/canon/mod.rs` — `pi_encode_inner` and `claude_encode_inner`

These mirror `encode_dir` and need the same fix so the `TokenRegistry` L1
canonicalization produces the correct encoded home prefix on Windows:

```rust
fn pi_encode_inner(path: &Path) -> String {
    path.to_string_lossy()
        .trim_start_matches(|c| c == '/' || c == '\\')
        .replace(['/', '\\', ':'], "-")
    // /Users/bradmatic/Dev  →  Users-bradmatic-Dev
    // C:\Users\brad\Dev     →  C--Users-brad-Dev
}

fn claude_encode_inner(path: &Path) -> String {
    // Claude strips dots too; same Windows handling as Pi.
    path.to_string_lossy()
        .trim_start_matches(|c| c == '/' || c == '\\')
        .replace(['/', '\\', ':', '.'], "-")
    // /Users/bradmatic/Dev  →  Users-bradmatic-Dev
    // C:\Users\brad\Dev     →  C--Users-brad-Dev
}
```

Note: `claude_encode_inner` is used only for `TokenRegistry` L1
canonicalization (matching tokens in encoded directory names), not for
generating the directory name itself.  The leading-dash / no-leading-dash
distinction for Claude is handled in `ClaudeAgent::encode_dir` above.

#### 2c. `src/canon/mod.rs` — `try_canonicalize_path`

The L2 path canonicalization checks `s.starts_with(home) && s[home.len()..].starts_with('/')`.  On Windows the separator after the home prefix is `\`:

```rust
// After:
let sep_ok = rest.starts_with('/') || rest.starts_with('\\');
```

#### 2d. `src/canon/mod.rs` — `try_canonicalize_text` (L3)

The `replace_in_text` function checks `after.starts_with('/')` to confirm a
path boundary.  On Windows paths use `\`:

```rust
// replace_in_text boundary check:
let at_boundary = after.is_empty() || after.starts_with('/') || after.starts_with('\\');
```

Additionally, when the local home is Windows-style, `try_canonicalize_text`
must also scan for the backslash-normalized form of the home path so that
content containing `C:\Users\brad\...` is caught.

#### 2e. `src/canon/mod.rs` — `try_decanonicalize_text`

When the local home path is Windows-style (contains `\` or starts with a
drive letter), after replacing `{{SYNC_HOME}}` the result will be
`C:\Users\brad/Dev/foo` (mixed separators).  A post-step must normalize:

```rust
// After home-token replacement on a Windows home:
if local_home_is_windows {
    result = result.replace('/', "\\");
}
```

3. Add proptest round-trip tests using Windows-style home paths
   (`C:\\Users\\brad`) to exercise all of the above.

### Phase 3 — OS Abstractions

1. Extract a `Scheduler` trait from `src/scheduler/cron.rs`.
2. Implement `src/scheduler/task_scheduler.rs` for Windows (`schtasks.exe`).
3. Dispatch on `cfg!(target_os = "windows")` in `src/scheduler/mod.rs`.
4. Extract a `Lock` trait (or conditional compilation blocks) from the
   `flock`-based lock code in `src/git/mod.rs`.
5. Implement the named-mutex lock for Windows using `windows-sys`.

### Phase 4 — Config Path

1. Make `dirs::config_dir()` the primary resolver (already used for data dir).
2. Add `dirs` as an explicit dependency if not already direct.

### Phase 5 — CI & Release

1. Add `windows-latest` to `ci.yml` matrix.
2. Add Windows release job to `release.yml`.
3. Add `cargo-wix` to the release toolchain and commit `wix/main.wxs`.

### Phase 6 — Packages (post-release)

Submit Scoop and winget manifests after first tagged Windows release.

## 7. Known Gaps and Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Pi / Claude session paths on Windows are wrong | High | Research spike (§8) must confirm before any implementation |
| `windows-sys` API for named mutex is low-level | Medium | Encapsulate behind a `Lock` trait; unit-test with mock |
| `schtasks.exe` output format varies across Windows versions | Medium | Parse CSV output defensively; integration-test on multiple Windows versions |
| Backslash paths in JSONL content may use mixed separators | Medium | Normalize both `\` and `/` before canonicalization |
| Long path support (>260 chars) requires manifest | Low | Add `<longPathAware>true</longPathAware>` to the WiX manifest |
| `cargo deny` may flag `windows-sys` licences | Low | Pre-check and add to `deny.toml` allowlist |

## 8. Research Spike Required

**Before Phase 2 begins**, create `docs/research/004-windows-agent-paths.md`
to answer:

1. Where does Pi store session JSONL files on Windows?
2. Where does Claude Code store session JSONL files on Windows?
3. Are path separators in session JSONL fields forward-slash or backslash on
   Windows?
4. Does `git2` / libgit2 compiled for MSVC handle `C:\`-style repo paths
   without issue?
5. Does `cargo-deny` have any issues running on `windows-latest`?

## 9. Out of Scope

- WSL1 / WSL2 (Linux-in-Windows) — treated as Linux, already supported.
- Git Bash / MSYS2 — treated as Unix, likely works already.
- `aarch64-pc-windows-msvc` — deferred.
- Chocolatey — deferred after winget/Scoop.

## 10. Acceptance Criteria

1. `cargo build --target x86_64-pc-windows-msvc` succeeds with no errors.
2. `cargo test` passes on `windows-latest` CI.
3. A Windows machine can run `chronicle init`, `sync`, `push`, and `pull`
   against a remote that also has macOS or Linux machines syncing to it.
4. Paths in session content (L2 and L3) are correctly canonicalized from
   `C:\Users\<name>\...` to `{{SYNC_HOME}}/...` and de-canonicalized back to
   native form on each OS.
5. L1 directory names from Windows paths (drive letter stripped) match the
   expected `--Users-<name>-...--` / `-Users-<name>-...` format.
6. `chronicle schedule install/status/uninstall` creates, queries, and deletes
   a Windows Task Scheduler task.
7. Concurrent `chronicle sync` invocations on Windows correctly contend on the
   named mutex (one proceeds, the other detects lock held).
8. `chronicle doctor` reports correct results on Windows, including SSH key
   detection and remote reachability.
9. `x86_64-pc-windows-msvc` `.exe` is attached to GitHub Releases.
10. MSI installer installs `chronicle.exe` and adds it to `PATH`.
