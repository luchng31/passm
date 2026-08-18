//! passm-cli: agent-executable verification seam for the passm core.
//!
//! UI-free proof that derive/encrypt/decrypt/vault-CRUD work end-to-end.
//! Manual `--flag value` parsing — no clap, deps stay minimal.
//!
//! Subcommands:
//!   derive     --password <pw> --salt <hex>          print master+vault key hex
//!   encrypt    --in <file> --out <file> --password <pw>
//!   decrypt    --in <file> --out <file> --password <pw>
//!   vault-add  --vault <file> --password <pw> --title <t> --username <u>
//!              --password-value <p> --url <u> --notes <n>
//!   vault-list --vault <file> --password <pw>

mod commands;
mod error;

use error::CliError;
use std::collections::HashMap;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), CliError> {
    let Some(subcommand) = args.first() else {
        return Err(CliError::Usage(
            "usage: passm-cli <derive|encrypt|decrypt|vault-add|vault-list> ...".into(),
        ));
    };
    let flags = parse_flags(&args[1..])?;
    match subcommand.as_str() {
        "derive" => commands::derive(&flags),
        "encrypt" => commands::encrypt(&flags),
        "decrypt" => commands::decrypt(&flags),
        "vault-add" => commands::vault_add(&flags),
        "vault-list" => commands::vault_list(&flags),
        other => Err(CliError::Usage(format!("unknown subcommand: {other}"))),
    }
}

/// Parses flat `--flag value` pairs into a map (flags are stored with their
/// leading `--`, so `--password` and `--password-value` are distinct keys).
fn parse_flags(args: &[String]) -> Result<HashMap<String, String>, CliError> {
    let mut flags = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let flag = &args[i];
        if !flag.starts_with("--") {
            return Err(CliError::Usage(format!("unexpected argument: {flag}")));
        }
        let value = args
            .get(i + 1)
            .ok_or_else(|| CliError::Usage(format!("missing value for {flag}")))?;
        flags.insert(flag.clone(), value.clone());
        i += 2;
    }
    Ok(flags)
}
