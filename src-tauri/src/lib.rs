//! passm-app: Tauri 2 desktop shell (placeholder crate).
//!
//! Real Tauri setup lands in T11; this keeps the workspace buildable without
//! webkit2gtk system libraries.

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