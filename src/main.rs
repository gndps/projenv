use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command};

// ---------------------------------------------------------------------------
// JSON parsing helpers (no external crates)
// ---------------------------------------------------------------------------

/// Minimal JSON value type for our two schemas.
#[derive(Debug, Clone)]
enum JsonValue {
    Str(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
    Null,
}

struct Parser {
    input: Vec<char>,
    pos: usize,
}

impl Parser {
    fn new(s: &str) -> Self {
        Parser {
            input: s.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.input.get(self.pos).copied();
        self.pos += 1;
        c
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn expect(&mut self, ch: char) -> Result<(), String> {
        self.skip_ws();
        match self.next() {
            Some(c) if c == ch => Ok(()),
            Some(c) => Err(format!("expected '{}' got '{}'", ch, c)),
            None => Err(format!("expected '{}' got EOF", ch)),
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.skip_ws();
        self.expect('"')?;
        let mut s = String::new();
        loop {
            match self.next() {
                None => return Err("unterminated string".into()),
                Some('"') => break,
                Some('\\') => match self.next() {
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some('r') => s.push('\r'),
                    Some('"') => s.push('"'),
                    Some('\\') => s.push('\\'),
                    Some('/') => s.push('/'),
                    Some(c) => {
                        s.push('\\');
                        s.push(c);
                    }
                    None => return Err("unexpected EOF in escape".into()),
                },
                Some(c) => s.push(c),
            }
        }
        Ok(s)
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        self.skip_ws();
        match self.peek() {
            Some('"') => Ok(JsonValue::Str(self.parse_string()?)),
            Some('[') => self.parse_array(),
            Some('{') => self.parse_object(),
            Some('n') => {
                for ch in ['n', 'u', 'l', 'l'] {
                    self.expect(ch)?;
                }
                Ok(JsonValue::Null)
            }
            Some(c) => Err(format!("unexpected char '{}'", c)),
            None => Err("unexpected EOF".into()),
        }
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.expect('[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(']') {
            self.next();
            return Ok(JsonValue::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.peek() {
                Some(',') => {
                    self.next();
                }
                Some(']') => {
                    self.next();
                    break;
                }
                _ => return Err("expected ',' or ']' in array".into()),
            }
        }
        Ok(JsonValue::Array(items))
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.expect('{')?;
        let mut pairs = Vec::new();
        self.skip_ws();
        if self.peek() == Some('}') {
            self.next();
            return Ok(JsonValue::Object(pairs));
        }
        loop {
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(':')?;
            let val = self.parse_value()?;
            pairs.push((key, val));
            self.skip_ws();
            match self.peek() {
                Some(',') => {
                    self.next();
                }
                Some('}') => {
                    self.next();
                    break;
                }
                _ => return Err("expected ',' or '}' in object".into()),
            }
        }
        Ok(JsonValue::Object(pairs))
    }
}

fn parse_json(s: &str) -> Result<JsonValue, String> {
    let mut p = Parser::new(s);
    let v = p.parse_value()?;
    Ok(v)
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Project {
    alias: String,
    path: String,
}

impl Project {
    /// Return the path with $HOME substituted.
    fn resolved_path(&self) -> String {
        if self.path.contains("$HOME") {
            let home = env::var("HOME").unwrap_or_default();
            self.path.replace("$HOME", &home)
        } else {
            self.path.clone()
        }
    }
}

// ---------------------------------------------------------------------------
// JSON serialisation helpers
// ---------------------------------------------------------------------------

fn escape_json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

fn projects_to_json(projects: &[Project]) -> String {
    let mut s = String::from("[\n");
    for (i, p) in projects.iter().enumerate() {
        s.push_str(&format!(
            "  {{\"alias\": \"{}\", \"path\": \"{}\"}}",
            escape_json_str(&p.alias),
            escape_json_str(&p.path)
        ));
        if i + 1 < projects.len() {
            s.push(',');
        }
        s.push('\n');
    }
    s.push(']');
    s
}

fn profile_to_json(projects: &[Project]) -> String {
    let inner = projects_to_json(projects);
    // indent each line of inner by 2 spaces
    let indented: String = inner
        .lines()
        .map(|l| format!("  {}", l))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{{\n  \"projects\": {}\n}}", indented)
}

// ---------------------------------------------------------------------------
// JSON deserialisation helpers
// ---------------------------------------------------------------------------

fn json_to_projects(val: &JsonValue) -> Result<Vec<Project>, String> {
    match val {
        JsonValue::Array(items) => {
            let mut projects = Vec::new();
            for item in items {
                let p = json_object_to_project(item)?;
                projects.push(p);
            }
            Ok(projects)
        }
        _ => Err("expected JSON array".into()),
    }
}

fn json_object_to_project(val: &JsonValue) -> Result<Project, String> {
    match val {
        JsonValue::Object(pairs) => {
            let mut alias = None;
            let mut path = None;
            for (k, v) in pairs {
                match k.as_str() {
                    "alias" => {
                        if let JsonValue::Str(s) = v {
                            alias = Some(s.clone());
                        }
                    }
                    "path" => {
                        if let JsonValue::Str(s) = v {
                            path = Some(s.clone());
                        }
                    }
                    _ => {}
                }
            }
            Ok(Project {
                alias: alias.ok_or("missing 'alias' field")?,
                path: path.ok_or("missing 'path' field")?,
            })
        }
        _ => Err("expected JSON object for project entry".into()),
    }
}

fn json_to_profile_projects(val: &JsonValue) -> Result<Vec<Project>, String> {
    match val {
        JsonValue::Object(pairs) => {
            for (k, v) in pairs {
                if k == "projects" {
                    return json_to_projects(v);
                }
            }
            Err("profile JSON missing 'projects' key".into())
        }
        _ => Err("expected JSON object for profile".into()),
    }
}

// ---------------------------------------------------------------------------
// State file paths
// ---------------------------------------------------------------------------

fn state_dir() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".projenv")
}

fn projects_file() -> PathBuf {
    state_dir().join("projects.json")
}

fn active_file() -> PathBuf {
    state_dir().join("active")
}

fn profiles_dir() -> PathBuf {
    state_dir().join("profiles")
}

fn buffer_file() -> PathBuf {
    state_dir().join("buffer.json")
}

// ---------------------------------------------------------------------------
// I/O helpers
// ---------------------------------------------------------------------------

fn ensure_state_dir() -> Result<(), String> {
    let dir = state_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create state dir: {}", e))?;
    fs::create_dir_all(profiles_dir()).map_err(|e| format!("cannot create profiles dir: {}", e))?;
    Ok(())
}

fn load_projects() -> Result<Vec<Project>, String> {
    let f = projects_file();
    if !f.exists() {
        return Ok(Vec::new());
    }
    let s = fs::read_to_string(&f).map_err(|e| format!("cannot read projects.json: {}", e))?;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let val = parse_json(trimmed).map_err(|e| format!("parse error in projects.json: {}", e))?;
    json_to_projects(&val)
}

fn save_projects(projects: &[Project]) -> Result<(), String> {
    ensure_state_dir()?;
    let s = projects_to_json(projects);
    fs::write(projects_file(), s).map_err(|e| format!("cannot write projects.json: {}", e))
}

fn load_active() -> Option<String> {
    let f = active_file();
    if f.exists() {
        fs::read_to_string(&f).ok().map(|s| s.trim().to_string())
    } else {
        None
    }
}

fn save_active(alias: &str) -> Result<(), String> {
    ensure_state_dir()?;
    fs::write(active_file(), alias).map_err(|e| format!("cannot write active file: {}", e))
}

fn load_buffer() -> Result<Vec<String>, String> {
    let f = buffer_file();
    if !f.exists() {
        return Ok(Vec::new());
    }
    let s = fs::read_to_string(&f).map_err(|e| format!("cannot read buffer.json: {}", e))?;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let val = parse_json(trimmed).map_err(|e| format!("parse error in buffer.json: {}", e))?;
    match val {
        JsonValue::Array(items) => {
            let mut out = Vec::new();
            for item in items {
                if let JsonValue::Str(s) = item {
                    out.push(s);
                }
            }
            Ok(out)
        }
        _ => Err("buffer.json must be a JSON array".into()),
    }
}

fn save_buffer(buf: &[String]) -> Result<(), String> {
    ensure_state_dir()?;
    let mut s = String::from("[\n");
    for (i, item) in buf.iter().enumerate() {
        s.push_str(&format!("  \"{}\"", escape_json_str(item)));
        if i + 1 < buf.len() {
            s.push(',');
        }
        s.push('\n');
    }
    s.push(']');
    fs::write(buffer_file(), s).map_err(|e| format!("cannot write buffer.json: {}", e))
}

// ---------------------------------------------------------------------------
// Resolve alias-or-index helper
// ---------------------------------------------------------------------------

fn resolve_alias_or_index<'a>(
    projects: &'a [Project],
    arg: &str,
) -> Option<(usize, &'a Project)> {
    // Try as alias first
    if let Some(idx) = projects.iter().position(|p| p.alias == arg) {
        return Some((idx, &projects[idx]));
    }
    // Try as 1-based index (displayed as 1, 2, …)
    if let Ok(n) = arg.parse::<usize>() {
        if n >= 1 && n <= projects.len() {
            let idx = n - 1;
            return Some((idx, &projects[idx]));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn cmd_init(alias: &str) -> Result<(), String> {
    ensure_state_dir()?;
    let mut projects = load_projects()?;
    if projects.iter().any(|p| p.alias == alias) {
        return Err(format!("alias '{}' already exists", alias));
    }
    let cwd = env::current_dir().map_err(|e| format!("cannot get cwd: {}", e))?;
    let path = cwd.to_string_lossy().to_string();
    projects.push(Project {
        alias: alias.to_string(),
        path,
    });
    save_projects(&projects)?;
    eprintln!("registered '{}' -> {}", alias, projects.last().unwrap().path);
    Ok(())
}

fn cmd_list() -> Result<(), String> {
    let projects = load_projects()?;
    let active = load_active();
    if projects.is_empty() {
        println!("No projects registered. Use: projenv init <alias>");
        return Ok(());
    }
    for (i, p) in projects.iter().enumerate() {
        let marker = if active.as_deref() == Some(&p.alias) {
            "*"
        } else {
            " "
        };
        println!("{} {}  {} -> {}", marker, i + 1, p.alias, p.resolved_path());
    }
    Ok(())
}

fn cmd_activate(arg: &str) -> Result<(), String> {
    match arg {
        "git" => {
            let output = Command::new("git")
                .args(["rev-parse", "--show-toplevel"])
                .output()
                .map_err(|e| format!("failed to run git: {}", e))?;
            if !output.status.success() {
                let err = String::from_utf8_lossy(&output.stderr);
                return Err(format!("git error: {}", err.trim()));
            }
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            println!("cd {}", shell_quote(&path));
            return Ok(());
        }
        "poetry" => {
            let cwd = env::current_dir().map_err(|e| format!("cannot get cwd: {}", e))?;
            let found = walk_up_for_file(&cwd, "pyproject.toml");
            match found {
                Some(dir) => {
                    println!("cd {}", shell_quote(&dir.to_string_lossy()));
                    return Ok(());
                }
                None => return Err("pyproject.toml not found in any parent directory".into()),
            }
        }
        _ => {}
    }

    let projects = load_projects()?;
    match resolve_alias_or_index(&projects, arg) {
        Some((_, p)) => {
            let resolved = p.resolved_path();
            println!("cd {}", shell_quote(&resolved));
            save_active(&p.alias)?;
            Ok(())
        }
        None => Err(format!("no project found for '{}'", arg)),
    }
}

fn shell_quote(s: &str) -> String {
    // Wrap in single quotes, escaping any single quotes inside
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn walk_up_for_file(start: &Path, filename: &str) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(filename).exists() {
            return Some(current);
        }
        match current.parent() {
            Some(p) if p != current => current = p.to_path_buf(),
            _ => return None,
        }
    }
}

fn cmd_remove(arg: &str) -> Result<(), String> {
    let mut projects = load_projects()?;
    match resolve_alias_or_index(&projects, arg) {
        Some((idx, _)) => {
            let removed = projects.remove(idx);
            save_projects(&projects)?;
            // Clear active if it was the removed project
            if load_active().as_deref() == Some(&removed.alias) {
                let _ = fs::remove_file(active_file());
            }
            eprintln!("removed '{}'", removed.alias);
            Ok(())
        }
        None => Err(format!("no project found for '{}'", arg)),
    }
}

fn cmd_profile_create(name: &str) -> Result<(), String> {
    ensure_state_dir()?;
    let projects = load_projects()?;
    let path = profiles_dir().join(format!("{}.json", name));
    let s = profile_to_json(&projects);
    fs::write(&path, s).map_err(|e| format!("cannot write profile: {}", e))?;
    eprintln!("saved profile '{}' with {} project(s)", name, projects.len());
    Ok(())
}

fn cmd_profile_load(arg: &str) -> Result<(), String> {
    ensure_state_dir()?;
    let profiles = list_profiles()?;

    // Try as index first
    let profile_path = if let Ok(n) = arg.parse::<usize>() {
        if n >= 1 && n <= profiles.len() {
            profiles[n - 1].clone()
        } else {
            return Err(format!(
                "profile index {} out of range (1-{})",
                n,
                profiles.len()
            ));
        }
    } else {
        // Try as name
        let path = profiles_dir().join(format!("{}.json", arg));
        if !path.exists() {
            return Err(format!("profile '{}' not found", arg));
        }
        path
    };

    let s = fs::read_to_string(&profile_path)
        .map_err(|e| format!("cannot read profile: {}", e))?;
    let val = parse_json(s.trim()).map_err(|e| format!("parse error in profile: {}", e))?;
    let projects = json_to_profile_projects(&val)?;
    save_projects(&projects)?;
    let name = profile_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    eprintln!("loaded profile '{}' with {} project(s)", name, projects.len());
    Ok(())
}

fn list_profiles() -> Result<Vec<PathBuf>, String> {
    let dir = profiles_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| format!("cannot read profiles dir: {}", e))?
        .filter_map(|entry| entry.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
        .collect();
    files.sort();
    Ok(files)
}

fn cmd_profile_list() -> Result<(), String> {
    let profiles = list_profiles()?;
    if profiles.is_empty() {
        println!("No profiles. Use: projenv profile create <name>");
        return Ok(());
    }
    for (i, p) in profiles.iter().enumerate() {
        let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        println!("  {}  {}", i + 1, name);
    }
    Ok(())
}

fn cmd_profile_update(name_opt: Option<&str>) -> Result<(), String> {
    ensure_state_dir()?;
    let projects = load_projects()?;

    let profile_path = match name_opt {
        Some(name) => profiles_dir().join(format!("{}.json", name)),
        None => {
            // Use active project's profile? Fall back to finding a single profile.
            let profiles = list_profiles()?;
            if profiles.len() == 1 {
                profiles.into_iter().next().unwrap()
            } else {
                return Err(
                    "specify a profile name: projenv profile update <name>".into()
                );
            }
        }
    };

    if !profile_path.exists() {
        return Err(format!(
            "profile '{}' does not exist; use 'projenv profile create' first",
            profile_path.file_stem().unwrap_or_default().to_string_lossy()
        ));
    }

    let s = profile_to_json(&projects);
    fs::write(&profile_path, s).map_err(|e| format!("cannot write profile: {}", e))?;
    let name = profile_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    eprintln!("updated profile '{}' with {} project(s)", name, projects.len());
    Ok(())
}

fn cmd_cp(file_arg: &str) -> Result<(), String> {
    ensure_state_dir()?;
    // Resolve to absolute path
    let path = {
        let p = PathBuf::from(file_arg);
        if p.is_absolute() {
            p
        } else {
            let cwd = env::current_dir().map_err(|e| format!("cannot get cwd: {}", e))?;
            cwd.join(p)
        }
    };
    let path_str = path.to_string_lossy().to_string();
    let mut buf = load_buffer()?;
    buf.push(path_str.clone());
    save_buffer(&buf)?;
    eprintln!("added to buffer: {}", path_str);
    Ok(())
}

fn cmd_paste() -> Result<(), String> {
    let mut buf = load_buffer()?;
    if buf.is_empty() {
        return Err("buffer is empty".into());
    }
    let file = buf.pop().unwrap();
    save_buffer(&buf)?;
    println!("cp {} .", shell_quote(&file));
    Ok(())
}

fn cmd_pasteall() -> Result<(), String> {
    let buf = load_buffer()?;
    if buf.is_empty() {
        eprintln!("buffer is empty");
        return Ok(());
    }
    for file in &buf {
        println!("cp {} .", shell_quote(file));
    }
    Ok(())
}

fn cmd_list_files() -> Result<(), String> {
    let buf = load_buffer()?;
    if buf.is_empty() {
        println!("Buffer is empty.");
        return Ok(());
    }
    for (i, f) in buf.iter().enumerate() {
        println!("  {}  {}", i + 1, f);
    }
    Ok(())
}

fn cmd_init_shell() {
    println!(
        r#"pa() {{ eval "$(projenv activate $@)"; }}
po() {{ eval "$(projenv activate $1)"; code .; }}
px() {{ eval "$(projenv activate $1)"; code -r .; }}
pls() {{ projenv list; }}
pin() {{ projenv init "$@"; }}
prm() {{ projenv remove "$@"; }}
pload() {{ projenv profile load "$@"; }}
pcreate() {{ projenv profile create "$@"; }}
plist() {{ projenv profile list; }}"#
    );
}

fn print_help() {
    println!(
        r#"projenv - project directory bookmark manager

USAGE:
  projenv <COMMAND> [ARGS]

COMMANDS:
  init <alias>              Register current directory with alias
  list (ls)                 List all projects with index; active marked with *
  activate <alias|index>    Print 'cd /path' to stdout (use with eval)
    activate git            cd to git root
    activate poetry         cd to pyproject.toml directory
  remove <alias|index>      Remove a bookmark

  profile create <name>     Save current project list as named profile
  profile load <name|index> Load a profile (replaces current project list)
  profile list              List all profiles
  profile update [name]     Update profile from current project list

  cp <file>                 Add file path to buffer (FIFO stack)
  paste                     Print 'cp <file> .' for last buffered file (eval)
  pasteall                  Print cp commands for all buffered files
  list-files                List files in buffer

  init-shell                Print shell integration functions (eval this)
  help                      Show this help

SHELL INTEGRATION:
  Add to ~/.bashrc or ~/.zshrc:
    eval "$(projenv init-shell)"

  Then use: pa myapp   po myapp   px myapp   pls   pin alias   prm alias
"#
    );
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_help();
        process::exit(0);
    }

    let result = match args[1].as_str() {
        "init" => {
            if args.len() < 3 {
                Err("Usage: projenv init <alias>".into())
            } else {
                cmd_init(&args[2])
            }
        }
        "list" | "ls" => cmd_list(),
        "activate" => {
            if args.len() < 3 {
                Err("Usage: projenv activate <alias|index|git|poetry>".into())
            } else {
                cmd_activate(&args[2])
            }
        }
        "remove" | "rm" => {
            if args.len() < 3 {
                Err("Usage: projenv remove <alias|index>".into())
            } else {
                cmd_remove(&args[2])
            }
        }
        "profile" => {
            if args.len() < 3 {
                Err("Usage: projenv profile <create|load|list|update> [name]".into())
            } else {
                match args[2].as_str() {
                    "create" => {
                        if args.len() < 4 {
                            Err("Usage: projenv profile create <name>".into())
                        } else {
                            cmd_profile_create(&args[3])
                        }
                    }
                    "load" => {
                        if args.len() < 4 {
                            Err("Usage: projenv profile load <name|index>".into())
                        } else {
                            cmd_profile_load(&args[3])
                        }
                    }
                    "list" => cmd_profile_list(),
                    "update" => {
                        let name_opt = args.get(3).map(|s| s.as_str());
                        cmd_profile_update(name_opt)
                    }
                    sub => Err(format!("unknown profile subcommand '{}'", sub)),
                }
            }
        }
        "cp" => {
            if args.len() < 3 {
                Err("Usage: projenv cp <file>".into())
            } else {
                cmd_cp(&args[2])
            }
        }
        "paste" => cmd_paste(),
        "pasteall" => cmd_pasteall(),
        "list-files" => cmd_list_files(),
        "init-shell" => {
            cmd_init_shell();
            Ok(())
        }
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        cmd => Err(format!("unknown command '{}'. Run 'projenv help' for usage.", cmd)),
    };

    match result {
        Ok(()) => {}
        Err(e) => {
            let _ = writeln!(io::stderr(), "error: {}", e);
            process::exit(1);
        }
    }
}
