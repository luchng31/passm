//! passm-vault: vault data model (entries, metadata), serialization (serde), ids (uuid).
//!
//! Real vault model lands in T3; this is the scaffold placeholder.

/// Placeholder entry point.
pub fn placeholder() -> u8 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        assert_eq!(placeholder(), 1);
    }
}