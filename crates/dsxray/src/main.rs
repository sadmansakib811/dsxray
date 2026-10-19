//! `dsxray FILE…`  show what .DS_Store files contain.
//! `dsxray scan DIR`  find .DS_Store files before you publish a folder; exits 1 when it finds any.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde_json::json;

#[derive(Parser)]
#[command(name = "dsxray", version, about = "X-ray Apple's .DS_Store files", args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// .DS_Store files to show
    files: Vec<PathBuf>,
    /// Print JSON instead of a table
    #[arg(long, global = true)]
    json: bool,
    /// Only list the file names each .DS_Store reveals
    #[arg(long)]
    names: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Find .DS_Store files under folders (say, a website or a release) and the names they leak.
    /// Exits with 1 when it finds one, so it can stop a deploy in CI.
    Scan {
        /// Folders to search (default: the current folder)
        dirs: Vec<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Scan { mut dirs }) => {
            if dirs.is_empty() {
                dirs.push(PathBuf::from("."));
            }
            scan(&dirs, cli.json)
        }
        None if cli.files.is_empty() => {
            eprintln!("usage: dsxray FILE… | dsxray scan [DIR…]   (dsxray --help for more)");
            ExitCode::from(2)
        }
        None => show(&cli.files, cli.json, cli.names),
    }
}

fn show(files: &[PathBuf], as_json: bool, names_only: bool) -> ExitCode {
    let mut failed = false;
    let mut out = Vec::new();
    for path in files {
        let parsed =
            std::fs::read(path).map_err(|e| e.to_string()).and_then(|b| dsxray::parse(&b).map_err(|e| e.to_string()));
        match parsed {
            Ok(store) if as_json => {
                out.push(json!({ "path": path.display().to_string(), "store": dsxray::to_json(&store) }))
            }
            Ok(store) if names_only => store.file_names().iter().for_each(|n| say(&format!("{n}\n"))),
            Ok(store) => say(&report(path, &store)),
            Err(e) => {
                failed = true;
                if as_json {
                    out.push(json!({ "path": path.display().to_string(), "error": e }));
                } else {
                    eprintln!("{}: {e}", path.display());
                }
            }
        }
    }
    if as_json {
        let doc = if out.len() == 1 { out.remove(0) } else { json!(out) };
        say(&format!("{}\n", serde_json::to_string_pretty(&doc).unwrap_or_default()));
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

/// Print, and stop quietly when the reader goes away (`dsxray … | head`).
fn say(text: &str) {
    if std::io::stdout().write_all(text.as_bytes()).is_err() {
        std::process::exit(0);
    }
}

fn report(path: &Path, s: &dsxray::DsStore) -> String {
    let names = s.file_names();
    let mut out = String::new();
    let nodes = s.regions.iter().filter(|r| r.kind == "leaf" || r.kind == "branch").count();
    let _ = writeln!(
        out,
        "{}: {} bytes, {} records about {} names, in a tree of {nodes} node{}",
        path.display(),
        s.file_size,
        s.records.len(),
        names.len(),
        if nodes == 1 { "" } else { "s" }
    );
    if let Some(p) = &s.problem {
        let _ = writeln!(out, "warning: stopped early: {p}");
    }
    let _ = writeln!(out, "\nIt reveals these names ({}):", names.len());
    for n in &names {
        let _ = writeln!(out, "  {n}");
    }
    let _ = writeln!(out, "\n{:<28}  {:<4}  {:<44}  MEANING", "NAME", "CODE", "VALUE");
    for r in &s.records {
        let d = dsxray::describe(r);
        let _ = writeln!(
            out,
            "{:<28}  {:<4}  {:<44}  {}",
            clip(&r.name, 28),
            r.code,
            clip(&d.summary, 44),
            d.meaning.unwrap_or("?")
        );
    }
    out
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else { s.chars().take(max - 1).chain(['…']).collect() }
}

fn scan(dirs: &[PathBuf], as_json: bool) -> ExitCode {
    let mut found = Vec::new();
    for dir in dirs {
        walk(dir, &mut found);
    }
    found.sort();
    found.dedup();
    let place = dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join(", ");
    let mut report = Vec::new();
    let mut total = 0;
    for path in &found {
        match std::fs::read(path).map_err(|e| e.to_string()).and_then(|b| dsxray::parse(&b).map_err(|e| e.to_string()))
        {
            Ok(store) => {
                let names: Vec<String> = store.file_names().into_iter().map(String::from).collect();
                total += names.len();
                report.push(json!({ "path": path.display().to_string(), "names": names }));
            }
            Err(e) => report.push(json!({ "path": path.display().to_string(), "error": e })),
        }
    }
    if as_json {
        say(&format!(
            "{}\n",
            serde_json::to_string_pretty(&json!({ "searched": dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>(), "found": report }))
                .unwrap_or_default()
        ));
    } else if found.is_empty() {
        say(&format!("No .DS_Store files under {place}.\n"));
    } else {
        let mut out = format!(
            "Found {} .DS_Store file{} under {place} (revealing {total} names):\n",
            found.len(),
            if found.len() == 1 { "" } else { "s" },
        );
        for item in &report {
            let path = item["path"].as_str().unwrap_or_default();
            match item["names"].as_array() {
                Some(names) => {
                    let first: Vec<&str> = names.iter().filter_map(|n| n.as_str()).take(4).collect();
                    let more = if names.len() > 4 { ", …" } else { "" };
                    let _ = writeln!(out, "  {path}  {} names: {}{more}", names.len(), first.join(", "));
                }
                None => {
                    let _ = writeln!(out, "  {path}  unreadable: {}", item["error"].as_str().unwrap_or_default());
                }
            }
        }
        out.push_str("Delete them before you publish this folder, and add .DS_Store to .gitignore.\n");
        say(&out);
    }
    if found.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// Every .DS_Store under `dir`. Symlinks aren't followed (no loops), unreadable folders are skipped.
fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if entry.file_name() != ".git" {
                walk(&path, found);
            }
        } else if kind.is_file() && entry.file_name() == ".DS_Store" {
            found.push(path);
        }
    }
}
