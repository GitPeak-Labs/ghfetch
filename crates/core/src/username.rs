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
mod tests;
