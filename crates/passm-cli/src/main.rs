//! passm-cli: command-line entry point.
//!
//! Real CLI lands in T5; this is the scaffold placeholder.

fn main() {
    // Trivial scaffold entry — prints nothing sensitive.
    let _ = passm_crypto::placeholder();
    let _ = passm_vault::placeholder();
    let _ = passm_sync::placeholder();
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {
        assert_eq!(passm_crypto::placeholder(), 1);
        assert_eq!(passm_vault::placeholder(), 1);
        assert_eq!(passm_sync::placeholder(), 1);
    }
}