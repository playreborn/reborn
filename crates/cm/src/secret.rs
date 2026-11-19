use std::fmt;

/// A secret string such as a password, a refresh token or Steam Guard
/// machine data.
///
/// `Debug` never prints the value and there is no `Display`
/// implementation, so a secret cannot end up in logs by accident. Use
/// [`Secret::expose`] only at the point where the raw value is needed.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wraps a secret value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the raw value. Never log or print it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::Secret;

    #[test]
    fn debug_output_is_redacted() {
        let secret = Secret::new("hunter2");
        let printed = format!("{secret:?} {:#?}", Some(&secret));
        assert!(!printed.contains("hunter2"));
        assert!(printed.contains("<redacted>"));
        assert_eq!(secret.expose(), "hunter2");
    }
}
