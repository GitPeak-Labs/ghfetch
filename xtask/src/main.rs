use std::{
    env, io,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    let task = env::args().nth(1).unwrap_or_else(|| "preflight".to_owned());
    let root = workspace_root();

    let result = match task.as_str() {
        "preflight" => preflight(&root),
        other => {
            eprintln!("unknown task {other:?}\nusage: cargo preflight");
            Err(())
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(()) => ExitCode::FAILURE,
    }
}

fn preflight(root: &Path) -> Result<(), ()> {
    cargo(
        root,
        "cargo fmt --all --check",
        &["fmt", "--all", "--check"],
    )?;

    cargo(
        root,
        "clippy (core)",
        &[
            "clippy",
            "-p",
            "ghfetch-core",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
            "-A",
            "clippy::pedantic",
        ],
    )?;

    require_target("wasm32-unknown-unknown")?;
    cargo(
        root,
        "clippy (worker, wasm32)",
        &[
            "clippy",
            "-p",
            "ghfetch-worker",
            "--target",
            "wasm32-unknown-unknown",
            "--locked",
            "--",
            "-D",
            "warnings",
            "-A",
            "clippy::pedantic",
        ],
    )?;

    cargo(root, "cargo test --locked", &["test", "--locked"])?;

    wrangler_dry_run(root)?;

    println!("\n\x1b[32m✔ all preflight checks passed, safe to push\x1b[0m");
    Ok(())
}

fn cargo(root: &Path, label: &str, args: &[&str]) -> Result<(), ()> {
    step(label);
    run(root, "cargo", args)
}

fn require_target(target: &str) -> Result<(), ()> {
    let output = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output();

    let installed = match output {
        Ok(output) => String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line == target),
        Err(_) => return Ok(()),
    };

    if installed {
        Ok(())
    } else {
        fail(&format!(
            "missing target {target}, run: rustup target add {target}"
        ))
    }
}

fn wrangler_dry_run(root: &Path) -> Result<(), ()> {
    step("wrangler deploy --dry-run");

    let status = Command::new("wrangler")
        .args(["deploy", "--dry-run"])
        .env("WRANGLER_SEND_METRICS", "false")
        .env("CI", "true")
        .current_dir(root)
        .status();

    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => fail(&format!("wrangler exited with {status}")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fail("wrangler not found, install it: npm i -g wrangler")
        }
        Err(error) => fail(&format!("failed to run wrangler: {error}")),
    }
}

fn run(root: &Path, program: &str, args: &[&str]) -> Result<(), ()> {
    match Command::new(program).args(args).current_dir(root).status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => fail(&format!("{program} exited with {status}")),
        Err(error) => fail(&format!("failed to run {program}: {error}")),
    }
}

fn step(label: &str) {
    println!("\n\x1b[1m==> {label}\x1b[0m");
}

fn fail(message: &str) -> Result<(), ()> {
    eprintln!("\x1b[31m✘ {message}\x1b[0m");
    Err(())
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask is a direct child of the workspace root")
        .to_owned()
}
