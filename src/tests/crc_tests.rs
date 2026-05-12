use crate::agent::tools::crc::{crc32, crc32_hex};

#[test]
fn test_crc32_empty() {
    assert_eq!(crc32(b""), 0x00000000);
}

#[test]
fn test_crc32_hello() {
    assert_eq!(crc32(b"hello"), 0x3610A686);
    assert_eq!(crc32_hex(b"hello"), "3610a686");
}

#[test]
fn test_crc32_deterministic() {
    let a = crc32(b"same string");
    let b = crc32(b"same string");
    assert_eq!(a, b);
}

#[test]
fn test_crc32_different() {
    let a = crc32(b"hello");
    let b = crc32(b"world");
    assert_ne!(a, b);
}
