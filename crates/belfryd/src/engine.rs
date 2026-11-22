//! Running the bundled EasyTier binaries (`easytier-core`, `easytier-cli`).

use serde::Deserialize;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(windows)]
const EXE: &str = ".exe";
#[cfg(not(windows))]
const EXE: &str = "";

pub struct Engine {
    pub dir: PathBuf,
}

/// One row of `easytier-cli -o json peer list`.
#[derive(Deserialize, Debug, Clone)]
pub struct CliPeer {
    #[serde(default)]
    pub ipv4: String,
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub cost: String,
    #[serde(default)]
    pub lat_ms: String,
    #[serde(default)]
    pub tunnel_proto: String,
}

impl Engine {
    pub fn locate(explicit: Option<PathBuf>) -> Result<Engine, String> {
        let mut candidates = Vec::new();
        if let Some(p) = explicit {
            candidates.push(p);
        }
        if let Ok(p) = std::env::var("BELFRY_ENGINE_DIR") {
            candidates.push(p.into());
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join("engine"));
                candidates.push(dir.join("../lib/belfry/engine"));
            }
        }
        candidates.push("/opt/belfry/engine".into());
        for c in &candidates {
            if c.join(format!("easytier-core{EXE}")).is_file() {
                return Ok(Engine { dir: c.clone() });
            }
        }
        Err(format!("EasyTier engine not found (looked in {candidates:?})"))
    }

    pub fn core(&self) -> PathBuf {
        self.dir.join(format!("easytier-core{EXE}"))
    }

    pub fn cli(&self) -> PathBuf {
        self.dir.join(format!("easytier-cli{EXE}"))
    }

    pub fn version(&self) -> String {
        run_with_timeout(Command::new(self.core()).arg("--version"), Duration::from_secs(5))
            .map(|s| s.trim().trim_start_matches("easytier-core").trim().to_string())
            .unwrap_or_else(|e| format!("unavailable ({e})"))
    }

    pub fn spawn(&self, args: &[String], log: &Path) -> io::Result<Child> {
        let out = std::fs::File::create(log)?;
        let err = out.try_clone()?;
        let mut cmd = Command::new(self.core());
        cmd.args(args)
            .current_dir(&self.dir)
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err);
        crate::platform::tie_child_to_us(&mut cmd);
        let child = cmd.spawn()?;
        crate::platform::after_spawn(&child);
        Ok(child)
    }

    pub fn peers(&self, rpc_port: u16) -> Result<Vec<CliPeer>, String> {
        let out = run_with_timeout(
            Command::new(self.cli())
                .args(["-p", &format!("127.0.0.1:{rpc_port}"), "-o", "json", "peer", "list"]),
            Duration::from_secs(3),
        )?;
        serde_json::from_str(&out).map_err(|e| format!("unexpected engine answer: {e}"))
    }
}

/// Runs a short command and returns its stdout, killing it if it hangs.
pub fn run_with_timeout(cmd: &mut Command, limit: Duration) -> Result<String, String> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    crate::platform::hide_window(cmd);
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = child.wait_with_output().map_err(|e| e.to_string())?;
                if status.success() {
                    return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
                }
                let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
                return Err(if msg.is_empty() { format!("exit {status}") } else { msg });
            }
            Ok(None) if start.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("timed out".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// The most useful line at the end of an engine log, for showing in the app.
pub fn last_error(log: &Path) -> Option<String> {
    let data = std::fs::read(log).ok()?;
    let tail = &data[data.len().saturating_sub(8192)..];
    let text = strip_ansi(&String::from_utf8_lossy(tail));
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let pick = lines
        .iter()
        .rev()
        .find(|l| {
            let u = l.to_ascii_lowercase();
            u.contains("error") || u.contains("panic") || u.contains("failed")
        })
        .or(lines.last())?;
    Some(pick.chars().take(300).collect())
}

pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\u{1b}' {
            if it.peek() == Some(&'[') {
                it.next();
                for c2 in it.by_ref() {
                    if c2.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
