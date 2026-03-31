---
date_created: 2026-03-30
date_modified: 2026-03-30
status: active
audience: both
cross_references:
  - docs/001-architecture.md
  - docs/research/002-sync-performance-investigation.md
---

# ADR-001: Stale Lock Recovery After Sleep/Suspend

## Context

Chronicle uses an advisory `flock(2)` on `<repo-parent>/chronicle.lock` to
prevent concurrent sync processes from colliding on the Git index (the "Gap 2
fix" introduced in v0.4.2).  The lock is non-blocking: a second cron
invocation sees `EWOULDBLOCK`, prints a skip message, and exits cleanly.

**Problem:** When a machine enters a low-power / sleep / suspend state *while
a sync is in progress*, the kernel freezes the process but does **not** kill
it.  On wake the process resumes, but its network connections are typically
dead, leaving it hung on an I/O call indefinitely.  Because the process is
still alive, the `flock` is still held, and every subsequent cron invocation
exits with "Another sync is in progress — skipping this run."

In the observed incident, the lock persisted for **9 hours** until the user
manually killed the hung process and removed the lock file.

### Requirements

1. New cron invocations must be able to detect and break a stale lock without
   manual intervention.
2. The fix must not introduce a race between two *legitimately concurrent*
   processes (the non-blocking mutual exclusion guarantee must be preserved).
3. The staleness threshold should have a sensible default and be
   configurable for users with unusually long sync operations.

## Decision

### Lock file contents

When Chronicle acquires the lock it writes the **holder's PID and a UTC
timestamp** into the file:

```
<PID> <UNIX_TIMESTAMP>
```

Example: `48201 1743292800`

### Staleness detection on acquisition failure

When `flock()` returns `EWOULDBLOCK`, Chronicle reads the lock file and
performs two checks:

1. **Dead process** — `kill(pid, 0)` returns `ESRCH` (no such process).
   The holder crashed or was killed after the machine woke; the lock is
   orphaned.
2. **Age exceeded** — the lock's timestamp is older than a configurable
   maximum (`general.lock_timeout_secs`, default **300 seconds / 5 minutes**).
   This covers the sleep/suspend case where the process is alive but hung.

If **either** check is true the lock is considered stale.

### Breaking a stale lock

1. Close the current file handle (releases our failed `flock` attempt — we
   never held it).
2. **Remove** the lock file (`fs::remove_file`).
3. **Re-open** the lock file (creates a fresh one).
4. Attempt `flock()` again.  If it succeeds, write the new PID + timestamp
   and proceed.  If it fails (another process raced us and won), return
   `Ok(None)` as before.

The remove-then-reopen sequence is safe against races: if two processes both
detect staleness simultaneously, only one will win the subsequent `flock()`
— the other gets `EWOULDBLOCK` and skips normally.

### Configuration

New field in `[general]`:

```toml
[general]
lock_timeout_secs = 300   # default: 5 minutes
```

Set to `0` to disable staleness-based recovery (PID-based recovery still
applies).  Set to `-1` to disable lock recovery entirely (original v0.4.2
behaviour).

## Consequences

### Positive

- Machines recover from sleep/suspend automatically on the next cron tick.
- No manual intervention required for the common laptop-lid-close scenario.
- PID check catches post-crash orphan locks even without the timeout.

### Negative

- A sync that legitimately runs longer than `lock_timeout_secs` will have
  its lock broken by the next cron invocation.  The default of 300 s (5 min)
  is well above the observed worst-case sync time (~2.5 min pre-v0.4.2,
  < 10 s post-v0.4.2), but users with very large repositories may need to
  increase it.
- On non-Unix platforms the PID check is not available; only the timestamp
  check applies.

### Alternatives Considered

1. **Heartbeat file** — the holder periodically touches the lock file to
   prove liveness.  Rejected: adds a background thread or signal handler to
   every sync, increasing complexity for marginal benefit over the simpler
   timestamp approach.
2. **PID file without flock** — write PID, check on startup.  Rejected:
   classic PID-file race conditions; `flock` remains the primary mutual
   exclusion mechanism.
3. **Watchdog process** — a separate daemon monitors lock age.  Rejected:
   over-engineered for a single-binary CLI tool.
4. **SIGALRM / process timeout** — set an alarm so the sync process kills
   itself after N seconds.  Rejected: does not work while the process is
   frozen during suspend; the kernel does not deliver signals to frozen
   processes.
