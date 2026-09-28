use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

const MAX_LEN: usize = 39;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct Username(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid GitHub username")]
pub struct InvalidUsername;

impl Username {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Username {
    type Error = InvalidUsername;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        let name = raw.trim();

        let well_formed = !name.is_empty()
            && name.len() <= MAX_LEN
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !name.starts_with('-')
            && !name.ends_with('-')
            && !name.contains("--");

        if well_formed {
            Ok(Self(name.to_ascii_lowercase()))
        } else {
            Err(InvalidUsername)
        }
    }
}

impl FromStr for Username {
    type Err = InvalidUsername;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        raw.to_owned().try_into()
    }
}

impl AsRef<str> for Username {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Username {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
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
}
