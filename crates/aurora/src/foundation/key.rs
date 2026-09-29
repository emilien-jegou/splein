// Single responsibility: Stable identity key for element diffing and state preservation.

/// Stable identifier for element reconciliation and persistent state diffing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key(pub String);

impl Key {
    /// Constructs a new identity key from a string slice.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl<S: Into<String>> From<S> for Key {
    /// Converts a string-like type into a reconciliation Key.
    fn from(s: S) -> Self {
        Self(s.into())
    }
}
