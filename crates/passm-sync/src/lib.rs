//! passm-sync: git-backed sync (git2), remote metadata (serde, uuid).
//!
//! Real sync lands in T4; this is the scaffold placeholder.

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