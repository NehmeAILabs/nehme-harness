//! Tests for the `memory` feature.
//!
//! Run with: cargo test --features memory
//!
//! Each test injects its own temp `root` via the public `Mem` fields, so they
//! need no env, no clock, no rig, and run fully in parallel. `fresh` also fixes
//! a known `project` slug and pre-creates the project-scoped subdirs, so tests
//! can write files directly. Paths are built from the public `root`/`project`
//! fields (Mem's own helpers are private).

use crate::extras::memory::{
    MAX_INJECT_BYTES, Mem, WriteMode, WriteTarget, append_memory_block, compaction_heading,
    effective_reserve, flush_compaction_summary,
};
use std::fs;
use std::path::PathBuf;

fn fresh(tag: &str) -> Mem {
    let root = std::env::temp_dir().join(format!(
        "zsmem-{}-{}-{:?}",
        tag,
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&root);
    // Pre-create the project-scoped layout so tests can `fs::write` directly
    // (create_dir_all also makes the intermediate projects/<slug>/ dir).
    let pdir = root.join("projects").join("proj");
    fs::create_dir_all(pdir.join("daily")).unwrap();
    fs::create_dir_all(pdir.join("notes")).unwrap();
    Mem {
        root,
        project: "proj".into(),
        today: "2026-05-25".into(),
        yesterday: "2026-05-24".into(),
    }
}
fn cleanup(m: &Mem) {
    let _ = fs::remove_dir_all(&m.root);
}
fn pdir(m: &Mem) -> PathBuf {
    m.root.join("projects").join(&m.project)
}
fn memory_md(m: &Mem) -> PathBuf {
    m.root.join("MEMORY.md") // global, shared across projects
}
fn scratchpad(m: &Mem) -> PathBuf {
    pdir(m).join("SCRATCHPAD.md")
}
fn daily(m: &Mem, d: &str) -> PathBuf {
    pdir(m).join("daily").join(format!("{d}.md"))
}

/// True if any hit's file path contains `needle` (used to identify which file a
/// hit came from now that hits are structured rather than `path:\nbody` strings).
fn hit_path_contains(m: &Mem, query: &str, needle: &str) -> bool {
    m.search(query)
        .hits
        .iter()
        .any(|h| h.path.to_string_lossy().contains(needle))
}

// ---- store: write / context_block -------------------------------------------

#[test]
fn empty_store_returns_none() {
    let m = fresh("empty");
    assert!(m.context_block().is_none());
    cleanup(&m);
}

#[test]
fn long_term_always_injected() {
    let m = fresh("lt");
    m.write(
        WriteTarget::LongTerm,
        "- never push to main",
        WriteMode::Append,
        None,
    )
    .unwrap();
    assert!(m.context_block().unwrap().contains("never push to main"));
    cleanup(&m);
}

#[test]
fn append_keeps_single_trailing_newline_and_overwrite_replaces() {
    let m = fresh("w");
    m.write(WriteTarget::LongTerm, "a", WriteMode::Append, None)
        .unwrap();
    m.write(WriteTarget::LongTerm, "b", WriteMode::Append, None)
        .unwrap();
    assert_eq!(fs::read_to_string(memory_md(&m)).unwrap(), "a\nb\n");
    m.write(WriteTarget::LongTerm, "new", WriteMode::Overwrite, None)
        .unwrap();
    assert_eq!(fs::read_to_string(memory_md(&m)).unwrap(), "new");
    cleanup(&m);
}

#[test]
fn append_to_file_without_trailing_newline_inserts_one() {
    let m = fresh("nl");
    fs::write(memory_md(&m), "no newline").unwrap(); // pre-existing content w/o \n
    m.write(WriteTarget::LongTerm, "next", WriteMode::Append, None)
        .unwrap();
    assert_eq!(
        fs::read_to_string(memory_md(&m)).unwrap(),
        "no newline\nnext\n"
    );
    cleanup(&m);
}

#[test]
fn scratchpad_write_then_inject_open_items_only() {
    let m = fresh("sp");
    m.write(
        WriteTarget::Scratchpad,
        "- [ ] first task",
        WriteMode::Append,
        None,
    )
    .unwrap();
    m.write(
        WriteTarget::Scratchpad,
        "- [x] closed task",
        WriteMode::Append,
        None,
    )
    .unwrap();
    assert!(scratchpad(&m).exists());
    let b = m.context_block().unwrap();
    assert!(b.contains("first task"));
    assert!(!b.contains("closed task")); // closed items are not injected
    // overwrite rewrites the whole list
    m.write(
        WriteTarget::Scratchpad,
        "- [ ] only this",
        WriteMode::Overwrite,
        None,
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(scratchpad(&m)).unwrap(),
        "- [ ] only this"
    );
    cleanup(&m);
}

#[test]
fn scratchpad_filter_handles_indent_and_star_bullets() {
    let m = fresh("spf");
    fs::write(
        scratchpad(&m),
        "- [ ] open one\n- [x] closed\n  - [ ] indented open\n* [ ] star open\nplain line\n",
    )
    .unwrap();
    let b = m.context_block().unwrap();
    assert!(b.contains("open one") && b.contains("indented open") && b.contains("star open"));
    assert!(!b.contains("closed") && !b.contains("plain line"));
    cleanup(&m);
}

#[test]
fn daily_order_yesterday_before_today() {
    let m = fresh("ord");
    m.write(WriteTarget::Daily, "TODAYMARK", WriteMode::Append, None)
        .unwrap();
    fs::write(daily(&m, &m.yesterday), "YESTMARK").unwrap();
    let b = m.context_block().unwrap();
    assert!(b.find("YESTMARK").unwrap() < b.find("TODAYMARK").unwrap());
    assert!(b.contains("(today)"));
    cleanup(&m);
}

#[test]
fn notes_never_injected_but_searchable() {
    let m = fresh("note");
    m.write(
        WriteTarget::Note,
        "jose for edge compat",
        WriteMode::Overwrite,
        Some("auth"),
    )
    .unwrap();
    assert!(!m.context_block().unwrap_or_default().contains("jose")); // not injected
    let r = m.search("jose");
    assert!(r.hits.iter().any(|h| h.body.contains("jose"))); // but recallable
    cleanup(&m);
}

#[test]
fn note_name_traversal_rejected() {
    let m = fresh("trav");
    for bad in ["../escape", "sub/dir", ".hidden", "a.b", "", "  "] {
        assert!(
            m.write(WriteTarget::Note, "x", WriteMode::Overwrite, Some(bad))
                .is_err(),
            "should reject note name {bad:?}"
        );
    }
    assert!(
        m.write(
            WriteTarget::Note,
            "x",
            WriteMode::Overwrite,
            Some("good-name")
        )
        .is_ok()
    );
    cleanup(&m);
}

#[test]
fn context_block_truncates_cjk_without_panic() {
    let m = fresh("cjk");
    m.write(
        WriteTarget::LongTerm,
        &"記憶實作".repeat(MAX_INJECT_BYTES),
        WriteMode::Overwrite,
        None,
    )
    .unwrap();
    let b = m.context_block().unwrap(); // must not panic mid-character
    assert!(b.contains("[memory truncated]"));
    assert!(b.len() <= MAX_INJECT_BYTES + 128);
    cleanup(&m);
}

// ---- search -----------------------------------------------------------------

#[test]
fn search_returns_surrounding_context_and_merges() {
    let m = fresh("ctx");
    // match on the "jose" line; with ±3 context "unrelated tail" is far enough
    m.write(
        WriteTarget::Note,
        "intro\na1\na2\na3\nblah\nwe chose jose\nbecause edge is incompatible\nb1\nb2\nb3\nunrelated tail",
        WriteMode::Overwrite,
        Some("auth"),
    )
    .unwrap();
    let r = m.search("jose");
    let e = r
        .hits
        .iter()
        .find(|h| h.path.to_string_lossy().contains("auth"))
        .unwrap();
    assert!(e.body.contains("we chose jose"));
    assert!(e.body.contains("because edge is incompatible")); // +1 line after the match
    assert!(e.body.contains("blah")); // -1 line before the match, still near enough
    assert!(!e.body.contains("unrelated tail")); // outside the ±3 context window
    assert!(!e.filename_only); // this is a content hit
    cleanup(&m);
}

#[test]
fn search_filename_match_falls_back_to_preview() {
    let m = fresh("fn");
    // filename contains "websocket"; content does not
    m.write(
        WriteTarget::Note,
        "first line\nsecond line",
        WriteMode::Overwrite,
        Some("websocket-fix"),
    )
    .unwrap();
    let r = m.search("websocket");
    let e = r
        .hits
        .iter()
        .find(|h| h.path.to_string_lossy().contains("websocket-fix"))
        .expect("filename hit");
    assert!(e.filename_only);
    assert!(e.body.contains("(filename match)"));
    assert!(e.body.contains("first line")); // preview is non-empty
    cleanup(&m);
}

#[test]
fn search_clean_miss_returns_empty() {
    let m = fresh("miss");
    m.write(
        WriteTarget::Note,
        "body text",
        WriteMode::Overwrite,
        Some("misc"),
    )
    .unwrap();
    assert!(m.search("nonexistent-xyz").hits.is_empty());
    cleanup(&m);
}

#[test]
fn search_is_literal_not_regex() {
    // the query is escaped, so regex metacharacters match literally
    let m = fresh("lit");
    m.write(
        WriteTarget::Note,
        "formula a+b=c",
        WriteMode::Overwrite,
        Some("math"),
    )
    .unwrap();
    // "a+b" has no whitespace -> a single literal term, not a regex
    assert!(
        m.search("a+b")
            .hits
            .iter()
            .any(|h| h.body.contains("a+b=c"))
    );
    cleanup(&m);
}

#[test]
fn search_caps_at_max_blocks() {
    let m = fresh("cap");
    // 7 well-separated matches (8-line spacing so ±3 context windows don't merge) -> cap at 5
    let body = (0..7)
        .map(|i| format!("hit{i}\na\nb\nc\nd\ne\nf\ng"))
        .collect::<Vec<_>>()
        .join("\n");
    m.write(WriteTarget::Note, &body, WriteMode::Overwrite, Some("many"))
        .unwrap();
    let e = m
        .search("hit")
        .hits
        .into_iter()
        .find(|h| h.path.to_string_lossy().contains("many"))
        .unwrap();
    assert!(
        e.body.contains("hit0")
            && e.body.contains("hit1")
            && e.body.contains("hit2")
            && e.body.contains("hit3")
            && e.body.contains("hit4")
    );
    assert!(!e.body.contains("hit5") && !e.body.contains("hit6")); // capped at MAX_BLOCKS = 5
    cleanup(&m);
}

#[test]
fn search_ranks_more_distinct_terms_first() {
    let m = fresh("rank");
    // alpha hits both terms; beta hits only one
    m.write(
        WriteTarget::Note,
        "uses redis\nbinds a port",
        WriteMode::Overwrite,
        Some("alpha"),
    )
    .unwrap();
    m.write(
        WriteTarget::Note,
        "only a port here",
        WriteMode::Overwrite,
        Some("beta"),
    )
    .unwrap();
    let r = m.search("redis port");
    assert!(r.hits[0].path.to_string_lossy().contains("alpha"));
    assert_eq!(r.hits[0].matched_terms.len(), 2); // matched both terms
    assert!(hit_path_contains(&m, "redis port", "beta")); // beta still recalled
    cleanup(&m);
}

#[test]
fn search_ranks_memory_md_first() {
    let m = fresh("mm");
    m.write(
        WriteTarget::LongTerm,
        "deploy uses needle",
        WriteMode::Append,
        None,
    )
    .unwrap();
    m.write(
        WriteTarget::Note,
        "needle in a note",
        WriteMode::Overwrite,
        Some("misc"),
    )
    .unwrap();
    let r = m.search("needle");
    assert!(r.hits[0].is_memory_md);
    assert!(r.hits[0].path.to_string_lossy().contains("MEMORY.md"));
    cleanup(&m);
}

#[test]
fn search_render_includes_summary_and_matched_tags() {
    let m = fresh("rend");
    m.write(
        WriteTarget::Note,
        "uses redis\nbinds a port",
        WriteMode::Overwrite,
        Some("alpha"),
    )
    .unwrap();
    m.write(
        WriteTarget::Note,
        "only a port here",
        WriteMode::Overwrite,
        Some("beta"),
    )
    .unwrap();
    let out = m.search("redis port").render(MAX_INJECT_BYTES);
    assert!(out.contains("Searched 2 terms"));
    assert!(out.contains("redis(") && out.contains("port(")); // per-term counts
    assert!(out.contains("[matched: redis, port]")); // tags shown in query order
    // alpha (2 terms) is rendered before beta (1 term)
    assert!(out.find("alpha").unwrap() < out.find("beta").unwrap());
    cleanup(&m);
}

#[test]
fn search_render_caps_output_with_marker() {
    let m = fresh("trunc");
    let filler = "x".repeat(300);
    for i in 0..6 {
        m.write(
            WriteTarget::Note,
            &format!("needle here\n{filler}"),
            WriteMode::Overwrite,
            Some(&format!("note{i}")),
        )
        .unwrap();
    }
    let r = m.search("needle");
    assert_eq!(r.hits.len(), 6);
    // A tight cap forces most files to be dropped, with an explicit marker.
    let capped = r.render(700);
    assert!(capped.contains("search truncated"));
    // The uncapped render shows everything, so no marker.
    let full = m.search("needle").render(MAX_INJECT_BYTES);
    assert!(!full.contains("search truncated"));
    cleanup(&m);
}

#[test]
fn search_empty_query_returns_no_hits() {
    let m = fresh("blank");
    m.write(
        WriteTarget::Note,
        "anything",
        WriteMode::Overwrite,
        Some("misc"),
    )
    .unwrap();
    assert!(m.search("   ").hits.is_empty()); // whitespace-only -> no terms
    cleanup(&m);
}

// ---- injection ------------------------------------------------------------

#[test]
fn append_memory_block_rules() {
    // None: no-op
    let mut p = "BASE".to_string();
    append_memory_block(&mut p, None);
    assert_eq!(p, "BASE");

    // empty: no-op (an empty store leaves zero trace)
    let mut p = "BASE".to_string();
    append_memory_block(&mut p, Some(""));
    assert_eq!(p, "BASE");

    // non-empty: appended after a separator, with the preamble preserved
    let mut p = "BASE".to_string();
    append_memory_block(&mut p, Some("<memory>x</memory>"));
    assert_eq!(p, "BASE\n\n---\n\n<memory>x</memory>");
}

// ---- compaction flush -----------------------------------------------------

#[test]
fn flush_compaction_summary_persists_to_today() {
    let m = fresh("flush");
    flush_compaction_summary(&m, "did X and Y", Some(12));
    let today = fs::read_to_string(daily(&m, &m.today)).unwrap();
    assert!(today.contains("compaction summary (12 msgs)"));
    assert!(today.contains("did X and Y"));
    // today's log is injected, so the summary also appears in the context block
    assert!(m.context_block().unwrap().contains("did X and Y"));
    cleanup(&m);
}

#[test]
fn compaction_heading_with_and_without_count() {
    assert_eq!(compaction_heading(Some(8)), "compaction summary (8 msgs)");
    assert_eq!(compaction_heading(None), "compaction summary");
}

#[test]
fn multiple_compactions_stay_separated_and_ordered() {
    let m = fresh("multi");
    flush_compaction_summary(&m, "first summary", Some(10));
    flush_compaction_summary(&m, "second summary", Some(7));
    let log = fs::read_to_string(daily(&m, &m.today)).unwrap();
    // ordered by append time, kept as two distinct heading sections
    assert!(log.find("first summary").unwrap() < log.find("second summary").unwrap());
    assert!(log.find("(10 msgs)").unwrap() < log.find("(7 msgs)").unwrap());
    assert_eq!(log.matches("compaction summary (").count(), 2);
    cleanup(&m);
}

// ---- compaction budget ----------------------------------------------------

#[test]
fn effective_reserve_adds_block_estimate() {
    // no memory -> base unchanged
    assert_eq!(effective_reserve(1000, None), 1000);
    // some memory -> base + estimate_tokens(block) (chars/4)
    let block = "x".repeat(400);
    assert_eq!(effective_reserve(1000, Some(&block)), 1000 + 100);
    // never drops below base
    assert!(effective_reserve(1000, Some("tiny")) >= 1000);
}
