use std::fs;
use std::path::Path;

const BUILTIN_VARIABLE_NAMES: [&str; 3] =
    ["ORA_API_HOST", "ORA_API_KEY_LLM", "ORA_API_KEY_TRANSCRIPTION"];

const UPDATER_PLACEHOLDER: &str = "REPLACE_WITH_";

fn main() {
    println!("cargo::rustc-check-cfg=cfg(optimized)");
    println!("cargo::rerun-if-env-changed=OPT_LEVEL");

    let optimized = std::env::var("OPT_LEVEL").unwrap_or_default() != "0";

    if optimized {
        println!("cargo::rustc-cfg=optimized");
    }

    let dotenv_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.env");

    println!("cargo::rerun-if-changed={}", dotenv_path.display());

    let dotenv = fs::read_to_string(&dotenv_path).unwrap_or_default();

    for name in BUILTIN_VARIABLE_NAMES {
        println!("cargo::rerun-if-env-changed={name}");

        // A variable set in the environment wins over .env even when it is empty, so a build
        // such as the end-to-end one can leave out a value that the checkout's .env holds.
        let value =
            std::env::var(name).ok().or_else(|| dotenv_value(&dotenv, name)).unwrap_or_default();

        if optimized {
            if value.trim().is_empty() {
                println!("cargo::warning={name} is not set; this build ships without it built in");
            }
        }

        println!("cargo::rustc-env={}={}", name, value.trim());
    }

    let config_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");

    println!("cargo::rerun-if-changed={}", config_path.display());

    if optimized {
        let config = fs::read_to_string(&config_path).unwrap_or_default();

        if config.contains(UPDATER_PLACEHOLDER) {
            println!(
                "cargo::warning=tauri.conf.json still holds the updater placeholder; this build \
                 cannot update itself"
            );
        }
    }

    tauri_build::build();
}

fn dotenv_value(dotenv: &str, name: &str) -> Option<String> {
    dotenv
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == name)
        .map(|(_, value)| value.trim().trim_matches('"').trim_matches('\'').to_owned())
}
