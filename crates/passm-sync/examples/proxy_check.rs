//! Temporary manual check: prints what the sync engine would use as proxy.
use passm_sync::git_repo::proxy_from_env;

fn main() {
    match proxy_from_env() {
        Some(p) => println!("resolved proxy: {p}"),
        None => println!("no proxy resolved"),
    }
}
