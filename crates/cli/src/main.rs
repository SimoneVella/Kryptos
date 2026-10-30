//! `kryptos`: the command line. Holds no secrets and never reads the vault: each
//! request goes to the running Kryptos app over its Unix socket, and the app
//! checks that this exact bundled binary is on the other end and asks the user
//! to confirm in its window (see apps/desktop/src-tauri/src/projects.rs).
//!
//! `env run` is the safe way to work: variables only reach the command's
//! environment, so no `.env` sits on disk to leak or be committed.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use kryptos_core::envs;
use serde_json::{json, Value};
use zeroize::Zeroizing;

const MAX_REPLY: u64 = 2 * 1024 * 1024;

const USAGE: &str = "Kryptos: your projects' .env files, kept in the vault.

Usage:
  kryptos env list
  kryptos env run <project> <env> -- <command> [args...]
  kryptos env pull <project> <env> [--out <file>] [--force]
  kryptos env push <project> <env> [--file <file>]
  kryptos env example [--file <file>] [--out <file>]

`run` starts the command with the variables set and writes nothing to disk.
Kryptos must be open and unlocked; each read or write asks you to confirm there.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["env", rest @ ..] => match env(rest) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("kryptos: {e}");
                ExitCode::FAILURE
            }
        },
        _ => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
    }
}

fn env(args: &[&str]) -> Result<ExitCode, String> {
    match args {
        ["list"] => list().map(|()| ExitCode::SUCCESS),
        ["run", project, name, "--", command @ ..] if !command.is_empty() => run(project, name, command),
        ["pull", project, name, flags @ ..] => pull(project, name, flags).map(|()| ExitCode::SUCCESS),
        ["push", project, name, flags @ ..] => push(project, name, flags).map(|()| ExitCode::SUCCESS),
        ["example", flags @ ..] => example(flags).map(|()| ExitCode::SUCCESS),
        _ => Err(format!("unknown command\n\n{USAGE}")),
    }
}

/// The value after `--name`, if given.
fn flag<'a>(flags: &[&'a str], name: &str) -> Option<&'a str> {
    flags.iter().position(|f| *f == name).and_then(|i| flags.get(i + 1).copied())
}

fn list() -> Result<(), String> {
    let reply = request(&json!({ "op": "list" }))?;
    let items = reply["environments"].as_array().cloned().unwrap_or_default();
    if items.is_empty() {
        println!("No projects yet: add them in Kryptos, or use `kryptos env push`.");
    }
    for e in items {
        let tag = if e["production"].as_bool() == Some(true) { "  (production)" } else { "" };
        println!("{}  {}  {} variables{tag}", e["project"].as_str().unwrap_or(""), e["env"].as_str().unwrap_or(""), e["vars"]);
    }
    Ok(())
}

fn get(project: &str, name: &str) -> Result<Zeroizing<String>, String> {
    let reply = request(&json!({ "op": "get", "project": project, "env": name }))?;
    Ok(Zeroizing::new(reply["content"].as_str().unwrap_or_default().to_owned()))
}

fn run(project: &str, name: &str, command: &[&str]) -> Result<ExitCode, String> {
    let content = get(project, name)?;
    let mut cmd = Command::new(command[0]);
    cmd.args(&command[1..]).envs(envs::parse(&content));
    // Replacing this process keeps signals (Ctrl-C) and the exit code as they are.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(format!("cannot start {}: {}", command[0], cmd.exec()))
    }
    #[cfg(not(unix))]
    {
        let status = cmd.status().map_err(|e| format!("cannot start {}: {e}", command[0]))?;
        Ok(ExitCode::from(status.code().unwrap_or(1) as u8))
    }
}

fn pull(project: &str, name: &str, flags: &[&str]) -> Result<(), String> {
    let path = PathBuf::from(flag(flags, "--out").unwrap_or(".env"));
    let force = flags.contains(&"--force");
    let content = get(project, name)?;
    if !force && path.exists() {
        let same = std::fs::read_to_string(&path).is_ok_and(|old| old == *content);
        if same {
            println!("{} is already up to date", path.display());
            return Ok(());
        }
        return Err(format!("{} exists and is different: use --force to replace it, or `env push` to save it first", path.display()));
    }
    write_private(&path, &content)?;
    println!("Wrote {} ({} variables)", path.display(), envs::parse(&content).len());
    warn_if_tracked(&path);
    Ok(())
}

fn push(project: &str, name: &str, flags: &[&str]) -> Result<(), String> {
    let path = flag(flags, "--file").unwrap_or(".env");
    let content = Zeroizing::new(std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?);
    if content.len() > envs::MAX_LEN {
        return Err(format!("{path} is too large"));
    }
    request(&json!({ "op": "put", "project": project, "env": name, "content": content.as_str() }))?;
    println!("Saved {path} to {project} / {name}");
    Ok(())
}

/// Needs no vault: works on the local file, and the result holds no values.
fn example(flags: &[&str]) -> Result<(), String> {
    let from = flag(flags, "--file").unwrap_or(".env");
    let to = flag(flags, "--out").unwrap_or(".env.example");
    let content = Zeroizing::new(std::fs::read_to_string(from).map_err(|e| format!("cannot read {from}: {e}"))?);
    std::fs::write(to, envs::example(&content)).map_err(|e| format!("cannot write {to}: {e}"))?;
    println!("Wrote {to}");
    Ok(())
}

/// Readable by this user only, from the moment it exists.
fn write_private(path: &Path, content: &str) -> Result<(), String> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = f.set_permissions(std::fs::Permissions::from_mode(0o600));
    }
    f.write_all(content.as_bytes()).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn warn_if_tracked(path: &Path) {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).map_or_else(|| std::env::current_dir().unwrap_or_default(), Path::to_path_buf);
    let Some(file) = path.file_name().map(|f| f.to_string_lossy().into_owned()) else { return };
    if !envs::is_gitignored(&dir, &file) {
        eprintln!("Warning: {file} doesn't look ignored by git. Add it to .gitignore before committing.");
    }
}

#[cfg(unix)]
fn request(body: &Value) -> Result<Value, String> {
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let path = kryptos_core::paths::ipc_socket_path();
    let mut stream = UnixStream::connect(&path).map_err(|_| explain("app_not_running"))?;
    // Reads and writes wait for the user's answer in the app (170 s there).
    stream.set_read_timeout(Some(Duration::from_secs(180))).map_err(|e| e.to_string())?;
    let mut msg = body.clone();
    msg["type"] = json!("env");
    let mut line = msg.to_string();
    line.push('\n');
    stream.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    let mut reply = String::new();
    BufReader::new(stream.take(MAX_REPLY)).read_line(&mut reply).map_err(|e| e.to_string())?;
    let reply: Value = serde_json::from_str(&reply).map_err(|_| "bad reply from Kryptos".to_owned())?;
    if reply["ok"].as_bool() == Some(true) {
        Ok(reply)
    } else {
        Err(explain(reply["error"].as_str().unwrap_or("error")))
    }
}

#[cfg(not(unix))]
fn request(_body: &Value) -> Result<Value, String> {
    Err("not available on this system yet".into())
}

fn explain(code: &str) -> String {
    match code {
        "app_not_running" => "Kryptos isn't running: open it first.".into(),
        "locked" => "Kryptos is locked: unlock it, then try again.".into(),
        "untrusted_peer" => "Kryptos only accepts the `kryptos` command bundled with the app.".into(),
        "env_not_found" => "No such project or environment: see `kryptos env list`.".into(),
        "denied" => "Declined in Kryptos.".into(),
        "timeout" => "No answer in Kryptos in time.".into(),
        "busy" => "Kryptos is already waiting for you on another request.".into(),
        "rate_limited" => "Too many requests: wait a minute.".into(),
        other => format!("Kryptos error: {other}"),
    }
}
