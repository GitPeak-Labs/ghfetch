use super::*;

fn is_valid(raw: &str) -> bool {
    raw.parse::<Username>().is_ok()
}

#[test]
fn accepts_valid_names() {
    assert!(is_valid("torvalds"));
    assert!(is_valid("pring-nt"));
    assert!(is_valid("user123"));
    assert!(is_valid(&"a".repeat(39)));
}

#[test]
fn rejects_empty_and_whitespace_only() {
    assert!(!is_valid(""));
    assert!(!is_valid("   "));
}

#[test]
fn rejects_too_long() {
    assert!(!is_valid(&"a".repeat(40)));
}

#[test]
fn rejects_misplaced_hyphens() {
    assert!(!is_valid("-username"));
    assert!(!is_valid("username-"));
    assert!(!is_valid("user--name"));
}

#[test]
fn rejects_special_characters() {
    for raw in ["user@name", "user name", "user.name", "<script>", "üser"] {
        assert!(!is_valid(raw), "{raw} should be rejected");
    }
}

#[test]
fn normalizes_case_and_trims() {
    let a: Username = "AmaneKai".parse().unwrap();
    let b: Username = "  amanekai ".parse().unwrap();
    assert_eq!(a, b);
    assert_eq!(a.as_str(), "amanekai");
}
