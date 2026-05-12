use crate::ui::slash::add::resolve_path;
use std::path::PathBuf;

#[test]
fn test_resolve_path_absolute() {
    let result = resolve_path("/tmp/foo.txt");
    assert_eq!(result, PathBuf::from("/tmp/foo.txt"));
}

#[test]
fn test_resolve_path_relative_root() {
    let result = resolve_path("/");
    assert_eq!(result, PathBuf::from("/"));
}

#[test]
fn test_resolve_path_relative_is_under_cwd() {
    let result = resolve_path("bar.txt");
    let expected = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("bar.txt");
    assert_eq!(result, expected);
}

#[test]
fn test_resolve_path_empty_joins_cwd() {
    let result = resolve_path("");
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    assert_eq!(result, cwd);
}
