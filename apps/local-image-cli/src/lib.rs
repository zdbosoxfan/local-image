//! Headless Local Image command line. `run` is the whole program, so it can be
//! tested in-process as well as through the binary.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![expect(clippy::let_underscore_must_use, clippy::map_err_ignore, reason = "baseline 2026-10-25: unreviewed")]

use std::io::Write;
use std::path::{Path, PathBuf};

use photocraft_automation::library::tools::{LibraryReply, call_tool as library_tool};
use photocraft_automation::{AuthorizedWorkspace, BridgePolicy, Headless, LibraryHeadless, PhotocraftMcp, files, security};
use photocraft_io::ExportOptions;
use serde_json::{Value, json};

pub const USAGE: &str = "\
local-image-cli: headless Local Image

USAGE:
  local-image-cli convert <in> <out> [--format <ext>] [--quality <1-100>] [--tiff-layers]
      Convert between formats (.pcraft, .psd, .png, .jpg, .tif, .webp, .exr, …).
      TIFF output is flat unless --tiff-layers keeps the layers (Photoshop layer data).
      --quality sets the JPEG or WebP quality; a WebP written with a quality is lossy, without one lossless.
  local-image-cli info <file> [--compact]
      Print the document as JSON (size, mode, depth, layer tree).
  local-image-cli run (<file> | --new <json>) --cmd <id> [--params <json>] [--cmd …] [--out <file>] [--format <ext>] [--quality <1-100>] [--tiff-layers]
      Open a file, run engine commands in order, save the result. Each --params
      applies to the preceding --cmd. Prints each command's JSON result.
  local-image-cli batch --actions <actions.json> --in <dir> --out <dir> [--format <ext>] [--quality <1-100>] [--in-place] [--tiff-layers]
      Apply an action list to every image in a directory. Steps are [id, params] pairs,
      {\"command\": id, \"params\": {…}} objects or bare ids, as a recorded action or droplet stores them
      (a list, or wrapped in {\"actions\": …}, {\"steps\": …} or a droplet). An --out folder that is the
      --in folder is refused, as the results would replace the originals; --in-place allows it.
      Camera RAW files are included and default to .pcraft output unless --format is supplied.
      Unsupported files are reported as skipped; no supported inputs is an error.
  local-image-cli droplet <file.pcdroplet> <file-or-dir>… [--out <dir>]
      Run a droplet (File › Automate › Create Droplet) on images and folders.
  local-image-cli commands [--json] [--filter <text>]
      List the engine command registry.
  local-image-cli library import <file-or-dir>… --library <dir> [--mode add|copy|move] [--destination <dir>]
      [--organize <template>] [--rename <template>]
      Add photos to a Library folder (created if needed). Folders are scanned recursively. copy/move file the
      originals into --destination (default: the library's Originals/) by --organize (date, month, flat or a
      template such as {date:%Y}/{date:%Y%m%d}), named by --rename (e.g. {date:%Y%m%d}_{seq:3}).
  local-image-cli library develop <photos> [--set <control>=<value>]… [--settings <json>] [--preset <id> [--amount <0-200>]]
      Edit photos: --set a slider (ids from `library run --cmd develop.controls`), --settings merges a partial
      develop JSON, --preset applies a develop preset. Prints each photo's develop settings.
  local-image-cli library export <photos> (--out <file> | --dir <dir>) [develop flags] [--format <ext>] [--quality <1-100>]
      [--long-edge <px>] [--naming <template>]
      Export with the Library's export pipeline (default long edge 3000 px; --long-edge 0 = full size; format from
      --out's extension unless --format). Never writes over an original.
  local-image-cli library snapshot <photo> --out <file> [--size <px>] [develop flags]
      Render one photo with its develop settings (long edge --size, default 1600) to an image file.
  local-image-cli library run [<photos>] --cmd <id> [--params <json>] [--cmd …]
      Run Library commands in order (the named photos selected first); prints each command's JSON result.
  local-image-cli library commands [--json] [--filter <text>]
      List the Library command registry.
      <photos> are image/raw files and folders (imported first) and/or --id <n> for photos already in --library.
      Every library subcommand takes --library <dir> to work in a Library folder (edits are saved there); without
      it, files are opened in a temporary in-memory library.
  local-image-cli mcp [--bridge <127.0.0.1:port>] [--control-token <64-hex> | --control-token-file <path>]
      [--automation-read-root <dir>] [--automation-write-root <dir>] [--allow-destructive] [--allow-cloud-ai]
      Run the MCP server on stdio (headless engine, or bridge to a running `photocraft --control <port>`).
      Headless, it also serves the Library as `library_*` tools (files below the automation roots);
      --allow-destructive lets them run Library commands that cannot be undone (photo.deletePermanently,
      album.delete, keyword.delete/merge, preset deletes). In bridge mode, requests for stored API keys
      are always refused, and those that can start paid cloud AI generation need --allow-cloud-ai.
  local-image-cli serve [--port <port>] [--control-token <64-hex> | --control-token-file <path>]
      [--automation-read-root <dir>] [--automation-write-root <dir>]
      Keep one headless session open and answer JSON lines ({\"id\",\"method\",\"params\"}) on stdio,
      or on 127.0.0.1:<port>. Methods: engine.execute, jobs.list/cancel, engine.commands,
      doc.open/new/save/inspect/render/select/close, session.list, batch, methods
      (docs/control-protocol.md#headless-server).

  local-image-cli <subcommand> --help (or -h) prints this text. A flag the subcommand doesn't take is
  an error.
";

struct Args {
    positional: Vec<String>,
    flags: Vec<(String, Option<String>)>,
}

/// A subcommand: the flags it reads (taking a value, or given bare) and what it runs. A flag
/// outside these is a usage error, so a typo such as `--fromat` can't be silently ignored (#423).
struct Subcommand {
    name: &'static str,
    values: &'static [&'static str],
    bare: &'static [&'static str],
    run: fn(&Args, &mut dyn Write, &mut dyn Write) -> R,
}

const SUBCOMMANDS: &[Subcommand] = &[
    Subcommand { name: "convert", values: &["--format", "--quality"], bare: &["--tiff-layers"], run: convert },
    Subcommand { name: "info", values: &[], bare: &["--compact"], run: |a, out, _| info(a, out) },
    Subcommand { name: "run", values: &["--new", "--cmd", "--params", "--out", "--format", "--quality"], bare: &["--tiff-layers"], run: run_cmds },
    Subcommand { name: "batch", values: &["--actions", "--in", "--out", "--format", "--quality"], bare: &["--in-place", "--tiff-layers"], run: batch },
    Subcommand { name: "droplet", values: &["--out"], bare: &[], run: droplet },
    Subcommand { name: "commands", values: &["--filter"], bare: &["--json"], run: |a, out, _| commands(a, out) },
    Subcommand {
        name: "library import",
        values: &["--library", "--mode", "--destination", "--organize", "--rename"],
        bare: &[],
        run: |a, out, _| library_import(a, out),
    },
    Subcommand { name: "library develop", values: LIBRARY_DEVELOP_FLAGS, bare: &[], run: |a, out, _| library_develop(a, out) },
    Subcommand { name: "library export", values: LIBRARY_EXPORT_FLAGS, bare: &[], run: |a, out, _| library_export(a, out) },
    Subcommand { name: "library snapshot", values: LIBRARY_SNAPSHOT_FLAGS, bare: &[], run: |a, out, _| library_snapshot(a, out) },
    Subcommand { name: "library run", values: &["--library", "--id", "--cmd", "--params"], bare: &[], run: |a, out, _| library_run(a, out) },
    Subcommand { name: "library commands", values: &["--filter"], bare: &["--json"], run: |a, out, _| library_commands(a, out) },
    Subcommand {
        name: "mcp",
        values: &["--bridge", "--control-token", "--control-token-file", "--automation-read-root", "--automation-write-root"],
        bare: &["--allow-destructive", "--allow-cloud-ai"],
        run: |a, _, _| mcp(a),
    },
    Subcommand {
        name: "serve",
        values: &["--port", "--control-token", "--control-token-file", "--automation-read-root", "--automation-write-root"],
        bare: &[],
        run: |a, _, err| serve(a, err),
    },
];

/// The subcommand's arguments, or `None` when they ask for help (`--help` or `-h`).
fn parse(sub: &Subcommand, args: &[String]) -> Result<Option<Args>, String> {
    let mut a = Args { positional: Vec::new(), flags: Vec::new() };
    let unknown = |flag: &str| format!("unknown flag {flag} for {}", sub.name);
    let mut i = 0;
    while let Some(s) = args.get(i) {
        if s == "--help" || s == "-h" {
            return Ok(None);
        }
        if let Some((k, v)) = s.split_once('=').filter(|(k, _)| k.starts_with("--")) {
            if sub.bare.contains(&k) {
                return Err(format!("{k} takes no value"));
            }
            if !sub.values.contains(&k) {
                return Err(unknown(k));
            }
            a.flags.push((k.to_owned(), Some(v.to_owned())));
        } else if sub.values.contains(&s.as_str()) {
            let v = args.get(i + 1).ok_or_else(|| format!("{s} needs a value"))?;
            a.flags.push((s.clone(), Some(v.clone())));
            i += 1;
        } else if sub.bare.contains(&s.as_str()) {
            a.flags.push((s.clone(), None));
        } else if s.starts_with("--") {
            return Err(unknown(s));
        } else {
            a.positional.push(s.clone());
        }
        i += 1;
    }
    Ok(Some(a))
}

impl Args {
    fn get(&self, k: &str) -> Option<&str> {
        self.flags.iter().rev().find(|(f, _)| f == k).and_then(|(_, v)| v.as_deref())
    }
    fn has(&self, k: &str) -> bool {
        self.flags.iter().any(|(f, _)| f == k)
    }
}

type R = Result<(), String>;

/// The `local-image-cli` process: sizes the compute pool, then runs the command line (without
/// `argv[0]`) against stdout and stderr and returns the exit status. The standalone binary and
/// `local-image` started under the `local-image-cli` name both call this, so they behave the same.
pub fn process_main() -> i32 {
    // One compute pool sized from the hardware, before anything uses rayon (`LOCAL_IMAGE_THREADS`
    // and `RAYON_NUM_THREADS` override it).
    lightcraft_sysmem::init(lightcraft_sysmem::Overrides::default());
    let args: Vec<String> = std::env::args().skip(1).collect();
    run(&args, &mut std::io::stdout(), &mut std::io::stderr())
}

/// Run the CLI. Returns the process exit code (0 ok, 1 failure, 2 usage).
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let Some(cmd) = args.first() else {
        let _ = write!(err, "{USAGE}");
        return 2;
    };
    match cmd.as_str() {
        "-h" | "--help" | "help" => {
            let _ = write!(out, "{USAGE}");
            return 0;
        }
        "--version" | "version" => {
            let _ = writeln!(out, "local-image-cli {}", photocraft_engine::build_info::long_version());
            return 0;
        }
        _ => {}
    }
    // `library <action>` names a two-word subcommand.
    let (name, rest) = match (cmd.as_str(), args.get(1)) {
        ("library", Some(action)) if !action.starts_with('-') => (format!("library {action}"), &args[2..]),
        _ => (cmd.clone(), &args[1..]),
    };
    if name == "library" {
        let help = matches!(args.get(1).map(String::as_str), Some("--help" | "-h"));
        if help {
            let _ = write!(out, "{USAGE}");
            return 0;
        }
        let _ = writeln!(err, "error: library needs an action: import, develop, export, snapshot, run or commands\n\n{USAGE}");
        return 2;
    }
    let Some(sub) = SUBCOMMANDS.iter().find(|s| s.name == name) else {
        let _ = writeln!(err, "error: unknown command `{name}`\n\n{USAGE}");
        return 2;
    };
    let parsed = match parse(sub, rest) {
        Ok(Some(p)) => p,
        // `<subcommand> --help`: usage, without reading inputs or starting a server.
        Ok(None) => {
            let _ = write!(out, "{USAGE}");
            return 0;
        }
        Err(e) => {
            let _ = writeln!(err, "error: {e}\n\n{USAGE}");
            return 2;
        }
    };
    match (sub.run)(&parsed, out, err) {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(err, "error: {e}");
            1
        }
    }
}

fn export_opts(a: &Args) -> Result<ExportOptions, String> {
    // TIFF output is flat unless asked: scripted conversions keep predictable sizes.
    let mut o = ExportOptions { tiff_layers: a.has("--tiff-layers"), ..Default::default() };
    if let Some(q) = a.get("--quality") {
        let q = q.parse().ok().filter(|q| (1..=100).contains(q)).ok_or_else(|| format!("bad --quality `{q}`: expected a whole number from 1 to 100"))?;
        o.encode.jpeg_quality = q;
        // Asking for a quality asks for a lossy WebP; the default WebP stays lossless.
        o.encode.webp_quality = q;
        o.encode.webp_lossless = false;
    }
    Ok(o)
}

fn warn_all(err: &mut dyn Write, ws: &[String]) {
    for w in ws {
        let _ = writeln!(err, "warning: {w}");
    }
}

fn automation_workspace(args: &Args) -> Result<AuthorizedWorkspace, String> {
    AuthorizedWorkspace::new(args.get("--automation-read-root").map(Path::new), args.get("--automation-write-root").map(Path::new))
        .map_err(|error| error.to_string())
}

fn convert(a: &Args, _out: &mut dyn Write, err: &mut dyn Write) -> R {
    let [input, output] = a.positional.as_slice() else {
        return Err("convert needs <in> <out>".into());
    };
    let opts = export_opts(a)?;
    let o = files::open(Path::new(input)).map_err(|e| e.to_string())?;
    warn_all(err, &o.warnings);
    let ws = files::save(&o.document, Path::new(output), a.get("--format"), &opts, None).map_err(|e| e.to_string())?;
    warn_all(err, &ws);
    Ok(())
}

fn print_json(out: &mut dyn Write, v: &Value, compact: bool) -> R {
    let s = if compact { serde_json::to_string(v) } else { serde_json::to_string_pretty(v) }.map_err(|e| e.to_string())?;
    writeln!(out, "{s}").map_err(|e| e.to_string())
}

fn info(a: &Args, out: &mut dyn Write) -> R {
    let [file] = a.positional.as_slice() else {
        return Err("info needs <file>".into());
    };
    let mut h = Headless::trusted_local();
    let opened = h.open(Path::new(file)).map_err(|e| e.to_string())?;
    let mut doc = h.inspect(None).map_err(|e| e.to_string())?;
    doc["warnings"] = opened["warnings"].clone();
    doc["file"] = json!(file);
    // Drop per-session noise.
    if let Value::Object(m) = &mut doc {
        for k in ["history", "canUndo", "canRedo", "revision"] {
            m.remove(k);
        }
    }
    print_json(out, &doc, a.has("--compact"))
}

/// `(id, params)` pairs from repeated `--cmd`/`--params` flags.
fn command_list(a: &Args) -> Result<Vec<(String, Value)>, String> {
    let mut cmds: Vec<(String, Value)> = Vec::new();
    for (k, v) in &a.flags {
        match (k.as_str(), v) {
            ("--cmd", Some(id)) => cmds.push((id.clone(), json!({}))),
            ("--params", Some(p)) => {
                let last = cmds.last_mut().ok_or("--params must follow a --cmd")?;
                last.1 = serde_json::from_str(p).map_err(|e| format!("bad --params JSON for `{}`: {e}", last.0))?;
            }
            _ => {}
        }
    }
    Ok(cmds)
}

fn run_cmds(a: &Args, out: &mut dyn Write, err: &mut dyn Write) -> R {
    let opts = export_opts(a)?;
    let mut h = Headless::trusted_local();
    match (a.positional.as_slice(), a.get("--new")) {
        ([file], None) => {
            let o = h.open(Path::new(file)).map_err(|e| e.to_string())?;
            let ws: Vec<String> = serde_json::from_value(o["warnings"].clone()).unwrap_or_default();
            warn_all(err, &ws);
        }
        ([], Some(new)) => {
            let p: Value = serde_json::from_str(new).map_err(|e| format!("bad --new JSON: {e}"))?;
            h.command_run("file.new", p).map_err(|e| format!("--new: {e}"))?;
        }
        _ => return Err("run needs exactly one of <file> or --new <json>".into()),
    }
    let cmds = command_list(a)?;
    if cmds.is_empty() && a.get("--out").is_none() {
        return Err("run needs at least one --cmd (or --out)".into());
    }
    for (id, params) in cmds {
        let r = h.command_run(&id, params).map_err(|e| format!("`{id}`: {e}"))?;
        print_json(out, &json!({"command": id, "result": r}), true)?;
    }
    if let Some(o) = a.get("--out") {
        let r = h.save(None, Some(Path::new(o)), a.get("--format"), &opts).map_err(|e| e.to_string())?;
        let ws: Vec<String> = serde_json::from_value(r["warnings"].clone()).unwrap_or_default();
        warn_all(err, &ws);
    }
    Ok(())
}

/// Parse an actions file: the steps of a recorded action or a droplet (`[id, params]` pairs,
/// `{"command": id, "params": {…}}` objects or bare ids; `"id"` is accepted for `"command"`), as a
/// list or wrapped in `{"actions": […]}`, `{"steps": […]}` or a droplet (#489).
pub fn parse_actions(text: &str) -> Result<Vec<(String, Value)>, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("actions JSON: {e}"))?;
    photocraft_engine::automate_cmds::parse_action(v.get("actions").unwrap_or(&v), "batch --actions").map_err(|e| e.to_string())
}

/// Whether `a` and `b` name the same existing folder, however they are spelt (relative, with a
/// trailing separator, through a symbolic link).
fn same_dir(a: &Path, b: &Path) -> bool {
    matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(a), Ok(b)) if a == b)
}

fn is_input(p: &Path) -> bool {
    let known = |e: &str| {
        matches!(e, "pcraft" | "psd" | "psb")
            || photocraft_codecs::raw_from_extension(e)
            || photocraft_codecs::from_extension(e).is_some_and(|f| photocraft_codecs::caps(f).read)
    };
    p.is_file() && p.extension().is_some_and(|e| known(&e.to_string_lossy().to_ascii_lowercase()))
}

fn batch(a: &Args, out: &mut dyn Write, err: &mut dyn Write) -> R {
    let actions_path = a.get("--actions").ok_or("batch needs --actions <file>")?;
    let in_dir = PathBuf::from(a.get("--in").ok_or("batch needs --in <dir>")?);
    let out_dir = PathBuf::from(a.get("--out").ok_or("batch needs --out <dir>")?);
    let text = std::fs::read_to_string(actions_path).map_err(|e| format!("{actions_path}: {e}"))?;
    let actions = parse_actions(&text)?;
    let opts = export_opts(a)?;
    // Results are saved as `<out>/<stem>.<ext>`, so an `--out` that is the `--in` folder would
    // replace the originals (#492).
    if !a.has("--in-place") && same_dir(&in_dir, &out_dir) {
        return Err(format!(
            "--out {} is the --in folder, so the results would replace the originals: choose another --out folder, or pass --in-place to overwrite them",
            out_dir.display()
        ));
    }
    let mut inputs = Vec::new();
    for entry in std::fs::read_dir(&in_dir).map_err(|e| format!("{}: {e}", in_dir.display()))? {
        let path = entry.map_err(|e| format!("{}: {e}", in_dir.display()))?.path();
        if is_input(&path) {
            inputs.push(path);
        } else if path.is_file() {
            let _ = writeln!(err, "SKIP  {}: unsupported input format", path.display());
        }
    }
    inputs.sort();
    if inputs.is_empty() {
        return Err(format!("{}: no supported input images", in_dir.display()));
    }
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let (mut ok, mut failed) = (0, 0);
    let mut written = photocraft_engine::file_cmds::OutputClaims::default();
    // `--format .jpg` names outputs `<stem>.jpg`, as `--format jpg` does (#490).
    let format = a.get("--format").map(|f| f.strip_prefix('.').unwrap_or(f));
    for input in &inputs {
        let ext = format.map(str::to_owned).unwrap_or_else(|| {
            let input_ext = input.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "png".into());
            if photocraft_codecs::raw_from_extension(&input_ext) { "pcraft".into() } else { input_ext }
        });
        let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let target = out_dir.join(format!("{stem}.{ext}"));
        let target_text = target.to_string_lossy();
        let r = (|| -> Result<Vec<String>, String> {
            written.check(&target_text)?;
            let mut h = Headless::trusted_local();
            h.open(input).map_err(|e| e.to_string())?;
            for (id, p) in &actions {
                h.command_run(id, p.clone()).map_err(|e| format!("`{id}`: {e}"))?;
            }
            let r = h.save(None, Some(&target), Some(&ext), &opts).map_err(|e| e.to_string())?;
            Ok(serde_json::from_value(r["warnings"].clone()).unwrap_or_default())
        })();
        match r {
            Ok(ws) => {
                ok += 1;
                written.record(&target_text, &input.to_string_lossy());
                let _ = writeln!(out, "ok    {} -> {}", input.display(), target.display());
                warn_all(err, &ws);
            }
            Err(e) => {
                failed += 1;
                let _ = writeln!(err, "FAIL  {}: {e}", input.display());
            }
        }
    }
    let _ = writeln!(out, "{ok} succeeded, {failed} failed");
    if failed > 0 { Err(format!("{failed} file(s) failed")) } else { Ok(()) }
}

fn droplet(a: &Args, out: &mut dyn Write, err: &mut dyn Write) -> R {
    let (file, inputs) = a.positional.split_first().ok_or("droplet needs <file.pcdroplet> and inputs")?;
    if inputs.is_empty() {
        return Err("droplet needs at least one input file or folder".into());
    }
    let mut p = json!({"droplet": file, "input": inputs});
    if let Some(o) = a.get("--out") {
        p["output"] = json!(o);
    }
    let mut s = photocraft_engine::Session::new();
    let r = s.execute("file.automate.runDroplet", p).map_err(|e| e.to_string())?;
    for f in r["files"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "ok    {}", f.as_str().unwrap_or_default());
    }
    let errors = r["errors"].as_array().cloned().unwrap_or_default();
    for e in &errors {
        let _ = writeln!(err, "FAIL  {}: {}", e["file"].as_str().unwrap_or_default(), e["error"].as_str().unwrap_or_default());
    }
    if errors.is_empty() { Ok(()) } else { Err(format!("{} file(s) failed", errors.len())) }
}

fn commands(a: &Args, out: &mut dyn Write) -> R {
    let h = Headless::new();
    let list = h.command_list();
    let needle = a.get("--filter").map(str::to_lowercase);
    let items: Vec<&Value> = list
        .as_array()
        .map(|v| v.iter().collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .filter(|c| {
            needle.as_ref().is_none_or(|n| format!("{} {}", c["id"].as_str().unwrap_or(""), c["label"].as_str().unwrap_or("")).to_lowercase().contains(n))
        })
        .collect();
    if a.has("--json") {
        return print_json(out, &Value::Array(items.into_iter().cloned().collect()), false);
    }
    for c in items {
        let menu = c["menu"].as_array().map(|m| m.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" > ")).unwrap_or_default();
        writeln!(out, "{:<44} {:<32} {}", c["id"].as_str().unwrap_or(""), c["label"].as_str().unwrap_or(""), menu).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn serve(a: &Args, err: &mut dyn Write) -> R {
    use std::sync::{Arc, Mutex};
    let h = Arc::new(Mutex::new(Headless::with_workspace(automation_workspace(a)?)));
    match a.get("--port") {
        Some(port) => {
            let port: u16 = port.parse().map_err(|_| format!("bad --port `{port}`"))?;
            let addr = format!("127.0.0.1:{port}");
            let (supplied, token_file) = security::token_inputs(a.get("--control-token").map(str::to_owned), a.get("--control-token-file").map(PathBuf::from));
            let token = security::server_token(supplied.as_deref(), token_file.as_deref()).map_err(|e| e.to_string())?;
            if let Some(path) = &token_file {
                let _ = writeln!(err, "local-image-cli control token file: {}", path.display());
            } else if supplied.is_none() {
                let _ = writeln!(err, "local-image-cli control token: {token}");
            }
            photocraft_automation::rpc::serve_tcp(&addr, h, token, |local| {
                let _ = writeln!(err, "local-image-cli serving on {local}");
            })
            .map_err(|e| e.to_string())
        }
        None => {
            let stdin = std::io::stdin();
            photocraft_automation::rpc::serve_lines(&h, stdin.lock(), std::io::stdout()).map_err(|e| e.to_string())
        }
    }
}

fn mcp(a: &Args) -> R {
    let server = mcp_server(a)?;
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().map_err(|e| e.to_string())?;
    rt.block_on(server.serve_stdio()).map_err(|e| e.to_string())
}

fn mcp_server(a: &Args) -> Result<PhotocraftMcp, String> {
    Ok(match a.get("--bridge") {
        Some(_) if a.has("--allow-destructive") => {
            return Err("--allow-destructive applies to the headless Library tools; it has no effect with --bridge".into());
        }
        Some(addr) => {
            let (supplied, token_file) = security::token_inputs(a.get("--control-token").map(str::to_owned), a.get("--control-token-file").map(PathBuf::from));
            let token = security::client_token(supplied.as_deref(), token_file.as_deref()).map_err(|e| e.to_string())?;
            PhotocraftMcp::bridge_with_policy(addr, &token, BridgePolicy { allow_cloud_ai: a.has("--allow-cloud-ai") }).map_err(|e| e.to_string())?
        }
        None if a.has("--allow-cloud-ai") => {
            return Err("--allow-cloud-ai applies to --bridge (the headless engine has no cloud AI)".into());
        }
        None => PhotocraftMcp::headless_with_workspace(automation_workspace(a)?).allow_destructive(a.has("--allow-destructive")),
    })
}

// Library (`library <action>`)

const LIBRARY_DEVELOP_FLAGS: &[&str] = &["--library", "--id", "--set", "--settings", "--preset", "--amount"];
const LIBRARY_EXPORT_FLAGS: &[&str] =
    &["--library", "--id", "--set", "--settings", "--preset", "--amount", "--out", "--dir", "--format", "--quality", "--long-edge", "--naming"];
const LIBRARY_SNAPSHOT_FLAGS: &[&str] = &["--library", "--id", "--set", "--settings", "--preset", "--amount", "--out", "--size"];

/// A local Library session: the `--library` folder when given, else in memory.
fn library_session(a: &Args) -> Result<LibraryHeadless, String> {
    let mut lib = LibraryHeadless::trusted_local();
    if let Some(dir) = a.get("--library") {
        lib.open_library(Path::new(dir))?;
    }
    Ok(lib)
}

fn library_json(r: Result<LibraryReply, String>) -> Result<Value, String> {
    r.map(|reply| reply.json().clone())
}

/// The photos a library subcommand names: positional files and folders (imported, or found
/// already in the library) and `--id <n>` flags, in that order.
fn library_photos(lib: &mut LibraryHeadless, a: &Args) -> Result<Vec<u64>, String> {
    let mut ids = Vec::new();
    if !a.positional.is_empty() {
        let r = lib.import(&json!({"paths": a.positional}))?;
        if let Some((path, error)) = r["failed"].as_array().into_iter().flatten().find_map(|f| Some((f.get(0)?.as_str()?, f.get(1)?.as_str()?))) {
            return Err(format!("{path}: {error}"));
        }
        let found = r["imported"].as_array().into_iter().chain(r["restored"].as_array()).flatten().filter_map(Value::as_u64);
        ids.extend(found);
        ids.extend(r["duplicates"].as_array().into_iter().flatten().filter_map(|d| d["existing"].as_u64()));
    }
    for (flag, value) in &a.flags {
        if flag == "--id" {
            let v = value.as_deref().unwrap_or_default();
            ids.push(v.parse().map_err(|_| format!("bad --id `{v}`: expected a photo id"))?);
        }
    }
    if ids.is_empty() {
        return Err("name the photos: image files or folders, and/or --id <n> for photos in --library".into());
    }
    Ok(ids)
}

/// Apply the develop flags (`--set`, `--settings`, `--preset`/`--amount`) to each photo.
fn library_apply_develop(lib: &mut LibraryHeadless, a: &Args, ids: &[u64]) -> R {
    let mut values = serde_json::Map::new();
    for (flag, value) in &a.flags {
        if flag == "--set" {
            let v = value.as_deref().unwrap_or_default();
            let (control, number) = v.split_once('=').ok_or_else(|| format!("bad --set `{v}`: expected <control>=<value>, e.g. light.exposure=0.5"))?;
            let number: f64 = number.trim().parse().map_err(|_| format!("bad --set `{v}`: the value must be a number"))?;
            values.insert(control.trim().to_string(), json!(number));
        }
    }
    let settings = a.get("--settings").map(|s| serde_json::from_str::<Value>(s).map_err(|e| format!("bad --settings JSON: {e}"))).transpose()?;
    let amount = a.get("--amount").map(|v| v.parse::<f64>().map_err(|_| format!("bad --amount `{v}`: expected 0 to 200"))).transpose()?;
    for &id in ids {
        if !values.is_empty() || settings.is_some() {
            let mut args = json!({"id": id});
            if !values.is_empty() {
                args["values"] = Value::Object(values.clone());
            }
            if let Some(s) = &settings {
                args["settings"] = s.clone();
            }
            library_tool(lib, "set_develop", &args).map_err(|e| format!("photo {id}: {e}"))?;
        }
        if let Some(preset) = a.get("--preset") {
            let mut args = json!({"preset": preset, "ids": [id]});
            if let Some(amount) = amount {
                args["amount"] = json!(amount);
            }
            library_tool(lib, "apply_preset", &args).map_err(|e| format!("photo {id}: {e}"))?;
        }
    }
    Ok(())
}

fn library_import(a: &Args, out: &mut dyn Write) -> R {
    if a.get("--library").is_none() {
        return Err("library import needs --library <dir> (the Library folder to import into)".into());
    }
    if a.positional.is_empty() {
        return Err("library import needs files or folders".into());
    }
    let mut lib = library_session(a)?;
    let mut args = json!({"paths": a.positional});
    for (flag, key) in [("--mode", "mode"), ("--destination", "destination"), ("--organize", "organize"), ("--rename", "rename")] {
        if let Some(v) = a.get(flag) {
            args[key] = json!(v);
        }
    }
    let r = library_json(library_tool(&mut lib, "import", &args))?;
    print_json(out, &r, false)
}

fn library_develop(a: &Args, out: &mut dyn Write) -> R {
    let mut lib = library_session(a)?;
    let ids = library_photos(&mut lib, a)?;
    library_apply_develop(&mut lib, a, &ids)?;
    for id in ids {
        let develop = library_json(library_tool(&mut lib, "get_develop", &json!({"id": id})))?;
        print_json(out, &json!({"id": id, "develop": develop}), true)?;
    }
    Ok(())
}

fn library_export(a: &Args, out: &mut dyn Write) -> R {
    let mut lib = library_session(a)?;
    let ids = library_photos(&mut lib, a)?;
    library_apply_develop(&mut lib, a, &ids)?;
    let mut args = json!({"ids": ids});
    match (a.get("--out"), a.get("--dir")) {
        (Some(file), None) if ids.len() == 1 => args["path"] = json!(file),
        (Some(_), None) => return Err("--out names one file; export several photos with --dir <dir>".into()),
        (None, Some(dir)) => args["dir"] = json!(dir),
        _ => return Err("library export needs exactly one of --out <file> or --dir <dir>".into()),
    }
    if let Some(f) = a.get("--format") {
        args["format"] = json!(f.strip_prefix('.').unwrap_or(f));
    }
    if let Some(q) = a.get("--quality") {
        let q: u8 = q.parse().ok().filter(|q| (1..=100).contains(q)).ok_or_else(|| format!("bad --quality `{q}`: expected a whole number from 1 to 100"))?;
        args["quality"] = json!(q);
    }
    if let Some(px) = a.get("--long-edge") {
        args["longEdge"] = json!(px.parse::<u32>().map_err(|_| format!("bad --long-edge `{px}`: expected pixels (0 = full size)"))?);
    }
    if let Some(n) = a.get("--naming") {
        args["naming"] = json!(n);
    }
    let r = library_json(library_tool(&mut lib, "export", &args))?;
    for f in r["files"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "ok    {}", f["path"].as_str().unwrap_or_default());
    }
    Ok(())
}

fn library_snapshot(a: &Args, out: &mut dyn Write) -> R {
    let file = a.get("--out").ok_or("library snapshot needs --out <file>")?;
    let size: u64 = match a.get("--size") {
        Some(s) => s.parse().map_err(|_| format!("bad --size `{s}`: expected pixels"))?,
        None => 1600,
    };
    let mut lib = library_session(a)?;
    let ids = library_photos(&mut lib, a)?;
    let [id] = ids.as_slice() else { return Err("library snapshot renders one photo".into()) };
    library_apply_develop(&mut lib, a, &ids)?;
    let img = lib.render(&json!({"id": id, "size": size}), size, photocraft_automation::library::tools::MAX_TRUSTED_RENDER)?;
    let ext = Path::new(file).extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let bytes = photocraft_automation::library::encode_image(&ext, &img, 92)?;
    lib.write_file(file, &bytes)?;
    let _ = writeln!(out, "ok    {file} ({}×{})", img.width, img.height);
    Ok(())
}

fn library_run(a: &Args, out: &mut dyn Write) -> R {
    let cmds = command_list(a)?;
    if cmds.is_empty() {
        return Err("library run needs at least one --cmd".into());
    }
    let mut lib = library_session(a)?;
    if !a.positional.is_empty() || a.get("--id").is_some() {
        let ids = library_photos(&mut lib, a)?;
        lib.call("engine.execute", json!({"command": "library.select", "params": {"ids": ids, "active": ids[0]}}))?;
    }
    for (id, params) in cmds {
        let r = lib.call("engine.execute", json!({"command": id, "params": params})).map_err(|e| format!("`{id}`: {e}"))?;
        print_json(out, &json!({"command": id, "result": r}), true)?;
    }
    Ok(())
}

fn library_commands(a: &Args, out: &mut dyn Write) -> R {
    let lib = LibraryHeadless::trusted_local();
    let list = lib.command_list();
    let needle = a.get("--filter").map(str::to_lowercase);
    let items: Vec<&Value> = list
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| {
            needle.as_ref().is_none_or(|n| format!("{} {}", c["id"].as_str().unwrap_or(""), c["label"].as_str().unwrap_or("")).to_lowercase().contains(n))
        })
        .collect();
    if a.has("--json") {
        return print_json(out, &Value::Array(items.into_iter().cloned().collect()), false);
    }
    for c in items {
        let menu = c["menu"].as_array().map(|m| m.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" > ")).unwrap_or_default();
        writeln!(out, "{:<44} {:<32} {}", c["id"].as_str().unwrap_or(""), c["label"].as_str().unwrap_or(""), menu).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli(args: &[&str]) -> (i32, String, String) {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run(&args, &mut out, &mut err);
        (code, String::from_utf8_lossy(&out).into_owned(), String::from_utf8_lossy(&err).into_owned())
    }

    fn photo(path: &Path, value: u8) {
        let img = photocraft_codecs::Image::from_u8(64, 48, photocraft_codecs::ChannelLayout::Rgba, vec![value; 64 * 48 * 4]).unwrap();
        std::fs::write(path, photocraft_codecs::encode(&img, photocraft_codecs::Format::Png, &Default::default()).unwrap()).unwrap();
    }

    /// The Library is reachable from the CLI — import into a library folder, develop, export
    /// and snapshot, by file and by id.
    #[test]
    fn library_subcommands_import_develop_export_snapshot() {
        let base = std::env::temp_dir().join(format!("cli-library-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("card")).unwrap();
        photo(&base.join("card/a.png"), 100);
        photo(&base.join("card/b.png"), 140);
        let s = |p: &str| base.join(p).to_string_lossy().into_owned();
        let (lib_dir, card) = (s("lib"), s("card"));

        let (code, out, err) = cli(&["library", "import", &card, "--library", &lib_dir]);
        assert_eq!(code, 0, "{err}");
        let imported: Value = serde_json::from_str(&out).unwrap();
        let ids: Vec<u64> = imported["imported"].as_array().unwrap().iter().filter_map(Value::as_u64).collect();
        assert_eq!(ids.len(), 2, "{out}");

        // Develop by id in the library; the edit persists for the next invocation.
        let id = ids[0].to_string();
        let (code, out, err) = cli(&["library", "develop", "--library", &lib_dir, "--id", &id, "--set", "light.exposure=0.75"]);
        assert_eq!(code, 0, "{err}");
        assert!(out.contains("\"exposure\":0.75"), "{out}");
        let (code, out, err) = cli(&["library", "run", "--library", &lib_dir, "--id", &id, "--cmd", "develop.get"]);
        assert_eq!(code, 0, "{err}");
        assert!(out.contains("\"exposure\":0.75"), "persisted: {out}");

        // Export several by id, one by file (in memory), and a snapshot.
        std::fs::create_dir_all(base.join("out")).unwrap();
        let ids_s: Vec<String> = ids.iter().map(u64::to_string).collect();
        let (code, out, err) = cli(&[
            "library",
            "export",
            "--library",
            &lib_dir,
            "--id",
            &ids_s[0],
            "--id",
            &ids_s[1],
            "--dir",
            &s("out"),
            "--format",
            "jpg",
            "--long-edge",
            "32",
        ]);
        assert_eq!(code, 0, "{err}");
        assert_eq!(out.lines().filter(|l| l.starts_with("ok")).count(), 2, "{out}");
        let (code, _, err) = cli(&["library", "export", &s("card/b.png"), "--out", &s("out/b-full.png"), "--long-edge", "0", "--set", "light.exposure=-1"]);
        assert_eq!(code, 0, "{err}");
        assert!(base.join("out/b-full.png").is_file());
        let (code, out, err) = cli(&["library", "snapshot", &s("card/a.png"), "--out", &s("out/a-snap.png"), "--size", "40"]);
        assert_eq!(code, 0, "{err}");
        assert!(out.contains("40×30"), "{out}");

        // Never over an original; usage errors.
        let (code, _, err) = cli(&["library", "export", &s("card/a.png"), "--out", &s("card/a.png")]);
        assert_eq!(code, 1);
        assert!(err.contains("original"), "{err}");
        assert_eq!(cli(&["library", "export", &s("card/a.png"), "--fromat", "png"]).0, 2);
        assert_eq!(cli(&["library"]).0, 2);
        assert_eq!(cli(&["library", "--help"]).0, 0);
        assert_eq!(cli(&["library", "develop"]).0, 1, "no photos named");
        let (code, out, _) = cli(&["library", "commands", "--filter", "photo.rate"]);
        assert_eq!(code, 0);
        assert!(out.contains("photo.rate"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
