// Grow-only set merge algorithm for JSONL session files (§5.2).
// Items here are consumed by US-007 (prefix verification) and the full sync
// pipeline (US-015/US-017). Allow dead-code until those callers are wired in.
#![allow(dead_code)]
//
// This module implements steps 1, 3, 4, and 5 of the merge algorithm.
// Step 2 (prefix verification / conflict detection) is added in US-007.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::merge::entry::{parse_entry, EntryKey, ParsedEntry};

// ── Public types ─────────────────────────────────────────────────────────────

/// A JSONL line that failed to parse and was skipped (§5.5).
#[derive(Debug, Clone)]
pub struct MalformedLine {
    /// Path of the file containing the malformed line.
    pub path: PathBuf,
    /// 1-based line number within the file.
    pub line_number: usize,
    /// A short snippet of the malformed content (at most 80 characters).
    pub snippet: String,
}

/// Output of a grow-only set merge operation.
#[derive(Debug)]
pub struct MergeOutput {
    /// The merged JSONL content, ready to write to disk.
    ///
    /// The session header is always first (if present), followed by all other
    /// entries sorted by timestamp ascending. Non-empty output ends with a
    /// trailing newline.
    pub content: String,
    /// Malformed lines that were skipped during parsing.
    pub malformed: Vec<MalformedLine>,
}

// ── Internal types ────────────────────────────────────────────────────────────

/// Which file an entry originated from, used as a stable-sort tie-breaker.
///
/// Remote entries (`0`) sort before local entries (`1`) when timestamps are
/// equal, providing deterministic output (§5.2 step 4c).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    Remote = 0,
    Local = 1,
}

/// A parsed entry annotated with its source and original line position.
#[derive(Debug, Clone)]
struct TaggedEntry {
    entry: ParsedEntry,
    source: Source,
    /// 0-based position in the source file; used as a final tie-breaker to
    /// preserve intra-file ordering for entries with identical timestamps.
    original_index: usize,
}

// ── File parsing ──────────────────────────────────────────────────────────────

/// Parse every non-empty line of `content` into [`TaggedEntry`] values.
///
/// Malformed lines are skipped; a warning is logged and a [`MalformedLine`]
/// record is appended to `malformed` (§5.5).
fn parse_file(
    content: &str,
    path: &Path,
    source: Source,
    malformed: &mut Vec<MalformedLine>,
) -> Vec<TaggedEntry> {
    let mut entries = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }

        match parse_entry(line) {
            Some(parsed) => entries.push(TaggedEntry {
                entry: parsed,
                source,
                original_index: idx,
            }),
            None => {
                let snippet: String = line.chars().take(80).collect();
                tracing::warn!(
                    file = %path.display(),
                    line = idx + 1,
                    snippet = %snippet,
                    "skipping malformed JSONL line"
                );
                malformed.push(MalformedLine {
                    path: path.to_owned(),
                    line_number: idx + 1,
                    snippet,
                });
            }
        }
    }

    entries
}

// ── Merge ─────────────────────────────────────────────────────────────────────

/// Perform a grow-only set merge of two JSONL session files (§5.2).
///
/// # Arguments
///
/// * `remote_content` — content of the remote (committed) version.
/// * `remote_path`    — path used for warning messages about the remote file.
/// * `local_content`  — content of the local working-tree version.
/// * `local_path`     — path used for warning messages about the local file.
///
/// # Algorithm
///
/// 1. Both files are parsed into entry sets keyed by [`EntryKey`].
/// 2. The union is computed: remote entries populate the set first; local
///    entries are added only when their key is not already present (remote wins
///    for identical keys with divergent content — verification/logging deferred
///    to US-007).
/// 3. The merged set is sorted: session header first, then all other entries
///    by timestamp ascending. Ties are broken by source (remote before local)
///    then by original line position (stable sort).
/// 4. The sorted entries are serialised back to JSONL.
///
/// Malformed lines are skipped with a warning (§5.5).
#[must_use]
pub fn merge_jsonl(
    remote_content: &str,
    remote_path: &Path,
    local_content: &str,
    local_path: &Path,
) -> MergeOutput {
    let mut malformed = Vec::new();

    let remote_entries = parse_file(remote_content, remote_path, Source::Remote, &mut malformed);
    let local_entries = parse_file(local_content, local_path, Source::Local, &mut malformed);

    // Build the union keyed by EntryKey.
    // Remote entries win when both sides carry the same key.
    let mut key_map: HashMap<EntryKey, TaggedEntry> = HashMap::new();

    for tagged in remote_entries {
        key_map.insert(tagged.entry.key.clone(), tagged);
    }
    for tagged in local_entries {
        // `entry` inserts only when the key is absent → remote wins.
        key_map.entry(tagged.entry.key.clone()).or_insert(tagged);
    }

    // Collect and sort: header first, then ascending timestamp, then source,
    // then original index (gives a strict total order — no two entries are
    // ever "equal" by this key).
    let mut merged: Vec<TaggedEntry> = key_map.into_values().collect();

    merged.sort_by(|a, b| {
        // Session header is always the first entry.
        let a_header = a.entry.key == EntryKey::Header;
        let b_header = b.entry.key == EntryKey::Header;
        match (a_header, b_header) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }

        // Primary: ascending timestamp (entries without a timestamp sort last).
        let ts_ord = match (&a.entry.timestamp, &b.entry.timestamp) {
            (Some(ta), Some(tb)) => ta.cmp(tb),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        };
        if ts_ord != std::cmp::Ordering::Equal {
            return ts_ord;
        }

        // Secondary: remote entries sort before local entries.
        let src_ord = a.source.cmp(&b.source);
        if src_ord != std::cmp::Ordering::Equal {
            return src_ord;
        }

        // Tertiary: original line position preserves intra-file ordering.
        a.original_index.cmp(&b.original_index)
    });

    // Serialise back to JSONL (one JSON object per line, trailing newline).
    let content = if merged.is_empty() {
        String::new()
    } else {
        let mut out = String::new();
        for tagged in &merged {
            out.push_str(&tagged.entry.raw);
            out.push('\n');
        }
        out
    };

    MergeOutput { content, malformed }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn remote_path() -> &'static Path {
        Path::new("remote.jsonl")
    }
    fn local_path() -> &'static Path {
        Path::new("local.jsonl")
    }

    // ── Empty files ────────────────────────────────────────────────────────

    #[test]
    fn merge_two_empty_files_produces_empty_output() {
        let out = merge_jsonl("", remote_path(), "", local_path());
        assert!(out.content.is_empty());
        assert!(out.malformed.is_empty());
    }

    #[test]
    fn merge_empty_remote_with_local_entries_returns_local_path() {
        let local = "{\"type\":\"session\"}\n{\"type\":\"message\",\"id\":\"m1\",\"timestamp\":\"2024-01-01T00:00:00Z\"}\n";
        let out = merge_jsonl("", remote_path(), local, local_path());
        let lines: Vec<&str> = out.content.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"session\""));
        assert!(out.malformed.is_empty());
    }

    #[test]
    fn merge_remote_entries_with_empty_local_returns_remote_path() {
        let remote = "{\"type\":\"session\"}\n{\"type\":\"message\",\"id\":\"m1\",\"timestamp\":\"2024-01-01T00:00:00Z\"}\n";
        let out = merge_jsonl(remote, remote_path(), "", local_path());
        let lines: Vec<&str> = out.content.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"session\""));
        assert!(out.malformed.is_empty());
    }

    // ── Header ordering ─────────────────────────────────────────────────────

    #[test]
    fn session_header_is_always_first_in_output() {
        // Header appears second in the remote file — must still end up first.
        let remote = concat!(
            "{\"type\":\"message\",\"id\":\"m1\",\"timestamp\":\"2024-01-01T01:00:00Z\"}\n",
            "{\"type\":\"session\",\"timestamp\":\"2024-01-01T00:00:00Z\"}\n",
        );
        let out = merge_jsonl(remote, remote_path(), "", local_path());
        let first = out.content.lines().next().unwrap();
        assert!(first.contains("\"session\""));
    }

    // ── Set-union semantics ─────────────────────────────────────────────────

    #[test]
    fn union_combines_disjoint_entry_sets() {
        let remote = concat!(
            "{\"type\":\"session\"}\n",
            "{\"type\":\"message\",\"id\":\"a\",\"timestamp\":\"2024-01-01T01:00:00Z\"}\n",
        );
        let local = concat!(
            "{\"type\":\"session\"}\n",
            "{\"type\":\"message\",\"id\":\"b\",\"timestamp\":\"2024-01-01T02:00:00Z\"}\n",
        );
        let out = merge_jsonl(remote, remote_path(), local, local_path());
        assert!(out.malformed.is_empty());
        let content = &out.content;
        // Both entries present.
        assert!(content.contains("\"a\""));
        assert!(content.contains("\"b\""));
        // Session header appears exactly once.
        assert_eq!(content.matches("\"session\"").count(), 1);
    }

    #[test]
    fn remote_wins_for_duplicate_entry_key() {
        let remote_entry =
            "{\"type\":\"message\",\"id\":\"x\",\"content\":\"remote\",\"timestamp\":\"2024-01-01T01:00:00Z\"}";
        let local_entry =
            "{\"type\":\"message\",\"id\":\"x\",\"content\":\"local\",\"timestamp\":\"2024-01-01T01:00:00Z\"}";
        let remote = format!("{remote_entry}\n");
        let local = format!("{local_entry}\n");
        let out = merge_jsonl(&remote, remote_path(), &local, local_path());
        assert!(out.content.contains("remote"));
        assert!(!out.content.contains("local"));
    }

    #[test]
    fn idempotent_merge_same_file_returns_same_entries() {
        let content = concat!(
            "{\"type\":\"session\"}\n",
            "{\"type\":\"message\",\"id\":\"1\",\"timestamp\":\"2024-01-01T01:00:00Z\"}\n",
            "{\"type\":\"message\",\"id\":\"2\",\"timestamp\":\"2024-01-01T02:00:00Z\"}\n",
        );
        let out = merge_jsonl(content, remote_path(), content, local_path());
        let merged_lines: Vec<&str> = out.content.lines().collect();
        let original_lines: Vec<&str> = content.lines().collect();
        assert_eq!(merged_lines.len(), original_lines.len());
    }

    // ── Timestamp ordering ──────────────────────────────────────────────────

    #[test]
    fn entries_sorted_by_timestamp_ascending() {
        let remote = concat!(
            "{\"type\":\"session\"}\n",
            "{\"type\":\"message\",\"id\":\"late\",\"timestamp\":\"2024-01-01T03:00:00Z\"}\n",
        );
        let local = concat!(
            "{\"type\":\"session\"}\n",
            "{\"type\":\"message\",\"id\":\"early\",\"timestamp\":\"2024-01-01T01:00:00Z\"}\n",
        );
        let out = merge_jsonl(remote, remote_path(), local, local_path());
        let lines: Vec<&str> = out.content.lines().collect();
        // Line 0: header, Line 1: early, Line 2: late.
        assert!(lines[0].contains("\"session\""));
        assert!(lines[1].contains("\"early\""));
        assert!(lines[2].contains("\"late\""));
    }

    #[test]
    fn entries_without_timestamp_sort_after_timestamped_entries() {
        let remote = concat!(
            "{\"type\":\"session\"}\n",
            "{\"type\":\"message\",\"id\":\"no_ts\"}\n",
        );
        let local = concat!(
            "{\"type\":\"session\"}\n",
            "{\"type\":\"message\",\"id\":\"has_ts\",\"timestamp\":\"2024-01-01T01:00:00Z\"}\n",
        );
        let out = merge_jsonl(remote, remote_path(), local, local_path());
        let lines: Vec<&str> = out.content.lines().collect();
        assert!(lines[0].contains("\"session\""));
        assert!(lines[1].contains("\"has_ts\""));
        assert!(lines[2].contains("\"no_ts\""));
    }

    // ── Stable sort: remote-first tie-break ─────────────────────────────────

    #[test]
    fn equal_timestamps_remote_entries_precede_local_path() {
        let ts = "2024-01-01T00:00:00Z";
        let remote = format!(
            "{{\"type\":\"session\"}}\n\
             {{\"type\":\"message\",\"id\":\"r\",\"timestamp\":\"{ts}\"}}\n"
        );
        let local = format!(
            "{{\"type\":\"session\"}}\n\
             {{\"type\":\"message\",\"id\":\"l\",\"timestamp\":\"{ts}\"}}\n"
        );
        let out = merge_jsonl(&remote, remote_path(), &local, local_path());
        let lines: Vec<&str> = out.content.lines().collect();
        assert!(lines[0].contains("\"session\""));
        // Remote entry "r" must come before local entry "l".
        let pos_r = lines.iter().position(|l| l.contains("\"r\"")).unwrap();
        let pos_l = lines.iter().position(|l| l.contains("\"l\"")).unwrap();
        assert!(
            pos_r < pos_l,
            "remote entry should sort before local for equal timestamps"
        );
    }

    // ── Malformed line handling (§5.5) ──────────────────────────────────────

    #[test]
    fn malformed_line_is_skipped_and_valid_lines_preserved() {
        let content = concat!(
            "{\"type\":\"session\"}\n",
            "NOT VALID JSON\n",
            "{\"type\":\"message\",\"id\":\"ok\",\"timestamp\":\"2024-01-01T01:00:00Z\"}\n",
        );
        let out = merge_jsonl(content, remote_path(), "", local_path());
        assert_eq!(out.malformed.len(), 1);
        assert_eq!(out.malformed[0].line_number, 2);
        assert_eq!(out.malformed[0].snippet, "NOT VALID JSON");
        // Valid entries still appear in output.
        assert!(out.content.contains("\"session\""));
        assert!(out.content.contains("\"ok\""));
    }

    #[test]
    fn malformed_line_recorded_with_correct_path_and_snippet() {
        let bad_content = "{bad json here}\n";
        let out = merge_jsonl(bad_content, remote_path(), "", local_path());
        assert_eq!(out.malformed.len(), 1);
        assert_eq!(out.malformed[0].path, remote_path());
        assert_eq!(out.malformed[0].line_number, 1);
        assert!(out.malformed[0].snippet.contains("bad json here"));
    }

    #[test]
    fn long_malformed_line_snippet_truncated_to_80_chars() {
        let long_bad: String = "x".repeat(200);
        let out = merge_jsonl(&long_bad, remote_path(), "", local_path());
        assert_eq!(out.malformed[0].snippet.len(), 80);
    }

    #[test]
    fn multiple_malformed_lines_all_recorded() {
        let content = "bad1\nbad2\nbad3\n";
        let out = merge_jsonl(content, remote_path(), "", local_path());
        assert_eq!(out.malformed.len(), 3);
    }

    // ── Trailing newline and output format ───────────────────────────────────

    #[test]
    fn non_empty_output_ends_with_trailing_newline() {
        let content = "{\"type\":\"session\"}\n";
        let out = merge_jsonl(content, remote_path(), "", local_path());
        assert!(out.content.ends_with('\n'));
    }

    #[test]
    fn empty_output_has_no_trailing_newline() {
        let out = merge_jsonl("", remote_path(), "", local_path());
        assert!(out.content.is_empty());
    }

    // ── Merge scenarios from §5.3 ────────────────────────────────────────────

    #[test]
    fn scenario_appended_file_local_has_more_entries() {
        // Repo has header + 2 entries; local has header + 3 entries (append-only).
        let remote = concat!(
            "{\"type\":\"session\"}\n",
            "{\"type\":\"message\",\"id\":\"1\",\"timestamp\":\"2024-01-01T01:00:00Z\"}\n",
            "{\"type\":\"message\",\"id\":\"2\",\"timestamp\":\"2024-01-01T02:00:00Z\"}\n",
        );
        let local = concat!(
            "{\"type\":\"session\"}\n",
            "{\"type\":\"message\",\"id\":\"1\",\"timestamp\":\"2024-01-01T01:00:00Z\"}\n",
            "{\"type\":\"message\",\"id\":\"2\",\"timestamp\":\"2024-01-01T02:00:00Z\"}\n",
            "{\"type\":\"message\",\"id\":\"3\",\"timestamp\":\"2024-01-01T03:00:00Z\"}\n",
        );
        let out = merge_jsonl(remote, remote_path(), local, local_path());
        let lines: Vec<&str> = out.content.lines().collect();
        // All 4 unique entries (header + 3) should be present.
        assert_eq!(lines.len(), 4);
        assert!(out.malformed.is_empty());
    }

    #[test]
    fn scenario_divergent_file_both_sides_appended() {
        // Both machines appended different entries from the same ancestor.
        let remote = concat!(
            "{\"type\":\"session\"}\n",
            "{\"type\":\"message\",\"id\":\"common\",\"timestamp\":\"2024-01-01T00:00:00Z\"}\n",
            "{\"type\":\"message\",\"id\":\"from_remote\",\"timestamp\":\"2024-01-01T02:00:00Z\"}\n",
        );
        let local = concat!(
            "{\"type\":\"session\"}\n",
            "{\"type\":\"message\",\"id\":\"common\",\"timestamp\":\"2024-01-01T00:00:00Z\"}\n",
            "{\"type\":\"message\",\"id\":\"from_local\",\"timestamp\":\"2024-01-01T03:00:00Z\"}\n",
        );
        let out = merge_jsonl(remote, remote_path(), local, local_path());
        assert!(out.malformed.is_empty());
        assert!(out.content.contains("\"common\""));
        assert!(out.content.contains("\"from_remote\""));
        assert!(out.content.contains("\"from_local\""));
        // "common" appears exactly once.
        assert_eq!(out.content.matches("\"common\"").count(), 1);
    }

    #[test]
    fn session_header_appears_exactly_once_when_present_in_both() {
        let remote = "{\"type\":\"session\",\"id\":\"s\"}\n";
        let local = "{\"type\":\"session\",\"id\":\"s\"}\n";
        let out = merge_jsonl(remote, remote_path(), local, local_path());
        assert_eq!(out.content.matches("\"session\"").count(), 1);
    }

    // ── Output ends with newline for every non-empty file ────────────────────

    #[test]
    fn content_without_trailing_newline_in_input_still_valid() {
        // Input lacks trailing newline — output should still be valid JSONL.
        let content = "{\"type\":\"session\"}";
        let out = merge_jsonl(content, remote_path(), "", local_path());
        assert!(out.content.ends_with('\n'));
    }
}
