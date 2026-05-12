//! Tests for the `archmd` feature.
//!
//! Run with: cargo test --features archmd
//!
//! Each test uses its own temp directory and asked-path file,
//! so they run in parallel safely.

use crate::extras::archmd;
use std::fs;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "zsarchmd-{}-{}-{:?}",
        tag,
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn asked_path(temp: &PathBuf) -> PathBuf {
    temp.join("asked.txt")
}

// ---------------------------------------------------------------------------
// should_ask_with_path
// ---------------------------------------------------------------------------

#[test]
fn should_ask_true_no_arch_not_asked() {
    let dir = temp_dir("should_ask_true");
    let ap = asked_path(&dir);
    assert!(archmd::should_ask_with_path(&dir, &ap));
}

#[test]
fn should_ask_false_arch_exists() {
    let dir = temp_dir("should_ask_false_arch");
    fs::write(dir.join("ARCHITECTURE.md"), "# test").unwrap();
    let ap = asked_path(&dir);
    assert!(!archmd::should_ask_with_path(&dir, &ap));
}

#[test]
fn should_ask_false_already_asked() {
    let dir = temp_dir("should_ask_false_asked");
    let ap = asked_path(&dir);
    archmd::record_asked_dir_with_path(&dir, &ap).unwrap();
    assert!(!archmd::should_ask_with_path(&dir, &ap));
}

#[test]
fn should_ask_false_both_arch_and_asked() {
    let dir = temp_dir("should_ask_false_both");
    let ap = asked_path(&dir);
    fs::write(dir.join("ARCHITECTURE.md"), "# test").unwrap();
    archmd::record_asked_dir_with_path(&dir, &ap).unwrap();
    assert!(!archmd::should_ask_with_path(&dir, &ap));
}

// ---------------------------------------------------------------------------
// has_been_asked_with_path
// ---------------------------------------------------------------------------

#[test]
fn has_been_asked_false_before_recording() {
    let dir = temp_dir("has_been_asked_false");
    let ap = asked_path(&dir);
    assert!(!archmd::has_been_asked_with_path(&dir, &ap));
}

#[test]
fn has_been_asked_true_after_recording() {
    let dir = temp_dir("has_been_asked_true");
    let ap = asked_path(&dir);
    archmd::record_asked_dir_with_path(&dir, &ap).unwrap();
    assert!(archmd::has_been_asked_with_path(&dir, &ap));
}

#[test]
fn has_been_asked_matches_canonical_path() {
    let dir = temp_dir("canonical");
    let ap = asked_path(&dir);
    archmd::record_asked_dir_with_path(&dir, &ap).unwrap();
    let canonical = dir.canonicalize().unwrap();
    assert!(archmd::has_been_asked_with_path(&canonical, &ap));
}

#[test]
fn has_been_asked_false_for_different_dir() {
    let dir_a = temp_dir("dir_a");
    let dir_b = temp_dir("dir_b");
    let ap = asked_path(&dir_a);
    archmd::record_asked_dir_with_path(&dir_a, &ap).unwrap();
    assert!(!archmd::has_been_asked_with_path(&dir_b, &ap));
}

#[test]
fn has_been_asked_false_empty_asked_file() {
    let dir = temp_dir("emptyfile");
    let ap = asked_path(&dir);
    fs::write(&ap, "").unwrap();
    assert!(!archmd::has_been_asked_with_path(&dir, &ap));
}

#[test]
fn has_been_asked_false_garbled_asked_file() {
    let dir = temp_dir("garbled");
    let ap = asked_path(&dir);
    fs::write(&ap, "/definitely/not/a/real/path\n").unwrap();
    assert!(!archmd::has_been_asked_with_path(&dir, &ap));
}

#[test]
fn has_been_asked_false_nonexistent_asked_file() {
    let dir = temp_dir("nonexistent");
    let ap = asked_path(&dir);
    // File never created — returns false
    assert!(!archmd::has_been_asked_with_path(&dir, &ap));
}

// ---------------------------------------------------------------------------
// record_asked_dir_with_path
// ---------------------------------------------------------------------------

#[test]
fn record_asked_dir_creates_file() {
    let dir = temp_dir("record_creates");
    let ap = asked_path(&dir);
    archmd::record_asked_dir_with_path(&dir, &ap).unwrap();
    let content = fs::read_to_string(&ap).unwrap();
    assert!(content.contains(dir.to_string_lossy().as_ref()));
}

#[test]
fn record_asked_dir_appends() {
    let dir_a = temp_dir("append_a");
    let dir_b = temp_dir("append_b");
    let ap = asked_path(&dir_a);
    archmd::record_asked_dir_with_path(&dir_a, &ap).unwrap();
    archmd::record_asked_dir_with_path(&dir_b, &ap).unwrap();
    let content = fs::read_to_string(&ap).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines.contains(&dir_a.to_str().unwrap()));
    assert!(lines.contains(&dir_b.to_str().unwrap()));
}

#[test]
fn record_asked_dir_idempotent() {
    let dir = temp_dir("idempotent");
    let ap = asked_path(&dir);
    archmd::record_asked_dir_with_path(&dir, &ap).unwrap();
    archmd::record_asked_dir_with_path(&dir, &ap).unwrap();
    let content = fs::read_to_string(&ap).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);
}

#[test]
fn record_asked_dir_preserves_existing_entries() {
    let dir_a = temp_dir("preserve_a");
    let dir_b = temp_dir("preserve_b");
    let ap = asked_path(&dir_a);
    archmd::record_asked_dir_with_path(&dir_a, &ap).unwrap();
    archmd::record_asked_dir_with_path(&dir_b, &ap).unwrap();
    let content = fs::read_to_string(&ap).unwrap();
    assert!(content.contains(&dir_a.to_str().unwrap()));
    assert!(content.contains(&dir_b.to_str().unwrap()));
}

#[test]
fn record_asked_dir_creates_parent_dirs() {
    let dir = temp_dir("parent_dirs");
    let sub_dir = dir.join("a").join("b").join("c");
    fs::create_dir_all(&sub_dir).unwrap();
    // asked_path inside a nested subdirectory
    let ap = dir.join("deep").join("nested").join("asked.txt");
    archmd::record_asked_dir_with_path(&sub_dir, &ap).unwrap();
    assert!(ap.exists());
}

// ---------------------------------------------------------------------------
// create_architecture_template
// ---------------------------------------------------------------------------

#[test]
fn create_template_writes_file() {
    let dir = temp_dir("create_tpl");
    archmd::create_architecture_template(&dir).unwrap();
    let arch_path = dir.join("ARCHITECTURE.md");
    assert!(arch_path.exists());
    let content = fs::read_to_string(&arch_path).unwrap();
    assert!(content.contains("# Architecture"));
    assert!(content.contains("## Contents to include"));
}

#[test]
fn create_template_idempotent_does_not_overwrite() {
    let dir = temp_dir("idem_tpl");
    let arch_path = dir.join("ARCHITECTURE.md");
    fs::write(&arch_path, "# custom architecture").unwrap();
    archmd::create_architecture_template(&dir).unwrap();
    let content = fs::read_to_string(&arch_path).unwrap();
    assert_eq!(content, "# custom architecture");
}

// ---------------------------------------------------------------------------
// ask_and_create (pure logic: non-interactive paths)
// ---------------------------------------------------------------------------

#[test]
fn ask_and_create_false_when_arch_exists() {
    let dir = temp_dir("ask_arch_exists");
    fs::write(dir.join("ARCHITECTURE.md"), "# test").unwrap();
    // No interactive prompt — should_ask returns false immediately
    assert!(!archmd::ask_and_create(&dir).unwrap());
}
