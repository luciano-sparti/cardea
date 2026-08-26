//! Property-style tests for Fenestra core functionality.
//! These use fixed test values to exercise formatting and sorting logic,
//! verifying correctness across a representative range of inputs.

use fenestra::fs::{format_size, format_permissions};

/// Test that format_size produces valid output for a range of byte sizes.
#[test]
fn prop_format_size_range() {
    let test_values = [
        0u64,
        1u64,
        500u64,
        1023u64,
        1024u64,
        1023 * 1024u64,
        1024 * 1024u64,
        1024 * 1024 * 1024u64,
        u64::MAX,
    ];

    for &bytes in &test_values {
        let result = format_size(bytes);
        assert!(
            result.contains(' '),
            "format_size({}) = '{}' should contain a space",
            bytes,
            result
        );
        assert!(
            result.ends_with(" B")
                || result.ends_with(" KB")
                || result.ends_with(" MB")
                || result.ends_with(" GB")
                || result.ends_with(" TB"),
            "format_size({}) = '{}' should end with known unit",
            bytes,
            result
        );
    }
}

/// Test that format_permissions produces 10-char strings for directory permissions.
#[test]
fn prop_format_permissions_dir_len() {
    let test_values = [0u32, 0o755, 0o644, 0o777, u32::MAX];

    for &perms in &test_values {
        let result = format_permissions(perms, true, false);
        assert_eq!(
            result.len(),
            10,
            "format_permissions dir len should be 10, got {} chars: '{}'",
            result.len(),
            result
        );
    }
}