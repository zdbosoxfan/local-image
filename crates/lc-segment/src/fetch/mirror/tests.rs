//! The mirror adapter over `li-ai::download` against a local HTTP server (std `TcpListener`;
//! never the internet): mirror fallback, redirects, chunked bodies, hash mismatches, size caps,
//! unpinned files, damaged files, cancel. (J8: resume, Range/416 and stall
//! tests went with the hand-written client; see docs/wip/CLAUDE-REPORT-j08-sam-defeature.md.)

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use super::*;

/// The served file: 300 kB of a deterministic pattern.
fn payload() -> Vec<u8> {
    (0..300_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect()
}

fn sha_hex(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|b| format!("{b:02x}")).collect()
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn pinned() -> FileSpec {
    let p = payload();
    FileSpec { name: "model.bin", size: Some(p.len() as u64), sha256: Some(leak(sha_hex(&p))), max: p.len() as u64 }
}

struct Server {
    base: String,
    hits: Arc<AtomicUsize>,
}

/// Serve on 127.0.0.1; the first path segment picks the behaviour.
fn server() -> Server {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    // plain http to this one in-process server only (li-ai's test hook)
    li_ai::download::allow_test_host(&l.local_addr().unwrap().to_string());
    // and to the closed port the fallback test lists (a connect failure, not a refused host)
    li_ai::download::allow_test_host("127.0.0.1:1");
    let hits = Arc::new(AtomicUsize::new(0));
    let h = hits.clone();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let h = h.clone();
            std::thread::spawn(move || handle(s, &h));
        }
    });
    Server { base, hits }
}

fn handle(mut s: TcpStream, hits: &AtomicUsize) {
    let n = hits.fetch_add(1, Ordering::SeqCst);
    let mut r = BufReader::new(s.try_clone().unwrap());
    let mut first = String::new();
    if r.read_line(&mut first).is_err() {
        return;
    }
    let mut range: Option<u64> = None;
    loop {
        let mut l = String::new();
        if r.read_line(&mut l).unwrap_or(0) == 0 || l.trim().is_empty() {
            break;
        }
        if let Some(v) = l.to_ascii_lowercase().strip_prefix("range: bytes=") {
            range = v.trim().trim_end_matches('-').parse().ok();
        }
    }
    let path = first.split_whitespace().nth(1).unwrap_or("/").to_string();
    let mode = path.trim_start_matches('/').split('/').next().unwrap_or("").to_string();
    let body = payload();
    let send = |s: &mut TcpStream, status: &str, headers: &str, data: &[u8]| {
        let _ = s.write_all(format!("HTTP/1.1 {status}\r\n{headers}Connection: close\r\n\r\n").as_bytes());
        let _ = s.write_all(data);
    };
    match mode.as_str() {
        "missing" => send(&mut s, "404 Not Found", "Content-Length: 9\r\n", b"not found"),
        "error" => send(&mut s, "500 Oops", "Content-Length: 25\r\n", b"<html>server error</html>"),
        "redirect" => send(&mut s, "302 Found", &format!("Location: {}\r\nContent-Length: 0\r\n", path.replacen("/redirect", "/good", 1)), b""),
        "wrong" => {
            let mut b = body.clone();
            b[1000] ^= 0xff;
            send(&mut s, "200 OK", &format!("Content-Length: {}\r\n", b.len()), &b)
        }
        "toolong" => {
            // no length announced, more bytes than the file can have
            let mut b = body.clone();
            b.extend_from_slice(&[7u8; 5000]);
            send(&mut s, "200 OK", "", &b)
        }
        "chunked" => {
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n");
            for c in body.chunks(70_001) {
                let _ = s.write_all(format!("{:x};ext=1\r\n", c.len()).as_bytes());
                let _ = s.write_all(c);
                let _ = s.write_all(b"\r\n");
            }
            let _ = s.write_all(b"0\r\nX-Trailer: 1\r\n\r\n");
        }
        "stall" => {
            // half the file, then nothing
            let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len()).as_bytes());
            let _ = s.write_all(&body[..body.len() / 2]);
            std::thread::sleep(Duration::from_secs(5));
        }
        "slow" => {
            let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len()).as_bytes());
            for c in body.chunks(1000) {
                if s.write_all(c).is_err() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        "flaky" if n.is_multiple_of(2) && range.is_none() => {
            // the first try breaks off after a third
            let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len()).as_bytes());
            let _ = s.write_all(&body[..body.len() / 3]);
        }
        "range416" if range.is_some() => send(&mut s, "416 Range Not Satisfiable", "Content-Length: 0\r\n", b""),
        "norange" | "range416" => send(&mut s, "200 OK", &format!("Content-Length: {}\r\n", body.len()), &body),
        _ => match range {
            Some(from) if from as usize >= body.len() => send(&mut s, "416 Range Not Satisfiable", "Content-Length: 0\r\n", b""),
            Some(from) => send(
                &mut s,
                "206 Partial Content",
                &format!("Content-Range: bytes {from}-{}/{}\r\nContent-Length: {}\r\n", body.len() - 1, body.len(), body.len() - from as usize),
                &body[from as usize..],
            ),
            None => send(&mut s, "200 OK", &format!("Content-Length: {}\r\n", body.len()), &body),
        },
    }
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("lc-sam3-fetch-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn quick() -> Options {
    Options { attempts: 2, token_env: None }
}

fn run(files: &[FileSpec], mirrors: &[String], dir: &Path, cancel: &AtomicBool) -> (Result<(), DownloadError>, Vec<Progress>) {
    let mut seen = Vec::new();
    let r = download(files, mirrors, dir, &quick(), cancel, &mut |p| seen.push(p.clone()));
    (r, seen)
}

/// Nothing but the named files in `dir` (no `.part` or error page left behind).
fn only(dir: &Path, names: &[&str]) {
    let mut have: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    have.sort();
    assert_eq!(have, names, "{}", dir.display());
}

#[test]
fn j08_falls_back_to_the_next_mirror_and_verifies() {
    let srv = server();
    let dir = tmp("fallback");
    let mirrors = vec![
        format!("{}/missing", srv.base),
        format!("{}/error", srv.base),
        "http://127.0.0.1:1/unreachable".to_string(),
        format!("{}/redirect", srv.base),
    ];
    let (r, seen) = run(&[pinned()], &mirrors, &dir, &AtomicBool::new(false));
    r.unwrap();
    assert_eq!(std::fs::read(dir.join("model.bin")).unwrap(), payload());
    let last = seen.last().unwrap();
    assert_eq!((last.done, last.total), (300_000, 300_000));
    // an error page never ends up in the file: nothing left over from the failed mirrors
    only(&dir, &["model.bin"]);
    // installed: nothing is fetched again
    let hits = srv.hits.load(Ordering::SeqCst);
    run(&[pinned()], &mirrors, &dir, &AtomicBool::new(false)).0.unwrap();
    assert_eq!(srv.hits.load(Ordering::SeqCst), hits);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn j08_a_hash_mismatch_is_deleted_and_the_next_mirror_used() {
    let srv = server();
    let dir = tmp("hash");
    let (r, _) = run(&[pinned()], &[format!("{}/wrong", srv.base)], &dir, &AtomicBool::new(false));
    let e = r.unwrap_err().to_string();
    assert!(e.contains("checksum"), "{e}");
    only(&dir, &[]);
    run(&[pinned()], &[format!("{}/wrong", srv.base), format!("{}/good", srv.base)], &dir, &AtomicBool::new(false)).0.unwrap();
    assert_eq!(std::fs::read(dir.join("model.bin")).unwrap(), payload());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn j08_damaged_files_are_replaced() {
    let srv = server();
    let dir = tmp("stale");
    std::fs::create_dir_all(&dir).unwrap();
    // a damaged copy under the real name (right size, wrong bytes)
    std::fs::write(dir.join("model.bin"), vec![1u8; 300_000]).unwrap();
    run(&[pinned()], &[format!("{}/good", srv.base)], &dir, &AtomicBool::new(false)).0.unwrap();
    assert_eq!(std::fs::read(dir.join("model.bin")).unwrap(), payload());
    only(&dir, &["model.bin"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn j08_chunked_bodies_and_unpinned_files() {
    let srv = server();
    let dir = tmp("chunked");
    run(&[pinned()], &[format!("{}/chunked", srv.base)], &dir, &AtomicBool::new(false)).0.unwrap();
    assert_eq!(std::fs::read(dir.join("model.bin")).unwrap(), payload());
    // an unpinned file is never downloaded (li-ai requires size + SHA-256)…
    let hits = srv.hits.load(Ordering::SeqCst);
    let f = FileSpec { name: "vocab.json", size: None, sha256: None, max: 1 << 20 };
    let e = run(std::slice::from_ref(&f), &[format!("{}/good", srv.base)], &dir, &AtomicBool::new(false)).0.unwrap_err();
    assert_eq!(e, DownloadError::Unpinned("vocab.json".into()));
    assert!(e.to_string().contains("model folder"), "{e}");
    assert_eq!(srv.hits.load(Ordering::SeqCst), hits, "nothing was requested");
    // …but one the user put in place is accepted when within its cap
    std::fs::write(dir.join("vocab.json"), b"{}").unwrap();
    run(std::slice::from_ref(&f), &[format!("{}/good", srv.base)], &dir, &AtomicBool::new(false)).0.unwrap();
    let small = FileSpec { max: 1, ..f };
    assert!(run(&[small], &[format!("{}/good", srv.base)], &dir, &AtomicBool::new(false)).0.is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn j08_a_server_sending_too_much_is_cut_off() {
    let srv = server();
    let dir = tmp("toolong");
    let e = run(&[pinned()], &[format!("{}/toolong", srv.base)], &dir, &AtomicBool::new(false)).0.unwrap_err().to_string();
    assert!(e.contains("larger than expected"), "{e}");
    only(&dir, &[]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn j08_a_host_off_the_allow_list_is_refused() {
    let dir = tmp("allow");
    let e = run(&[pinned()], &["https://mirror.invalid/sam3".to_string()], &dir, &AtomicBool::new(false)).0.unwrap_err().to_string();
    assert!(e.contains("unexpected host"), "{e}");
    only(&dir, &[]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn j08_cancel_stops_quickly_and_leaves_nothing() {
    let srv = server();
    let dir = tmp("cancel");
    let cancel = Arc::new(AtomicBool::new(false));
    let c = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(400));
        c.store(true, Ordering::SeqCst);
    });
    let t = Instant::now();
    let (r, _) = run(&[pinned()], &[format!("{}/slow", srv.base)], &dir, &cancel);
    assert_eq!(r, Err(DownloadError::Cancelled));
    assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
    only(&dir, &[]);
    // and the next run fetches it whole
    run(&[pinned()], &[format!("{}/good", srv.base)], &dir, &AtomicBool::new(false)).0.unwrap();
    assert_eq!(std::fs::read(dir.join("model.bin")).unwrap(), payload());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn j08_no_mirrors_is_a_clear_error_and_mirror_lists_are_parsed() {
    let dir = tmp("nomirror");
    let e = run(&[pinned()], &[], &dir, &AtomicBool::new(false)).0.unwrap_err();
    assert_eq!(e, DownloadError::NoMirrors);
    assert!(e.to_string().contains("no download location"));
    let file = std::env::temp_dir().join(format!("lc-sam3-mirrors-{}.txt", std::process::id()));
    std::fs::write(&file, "# mine\nhttps://b.example/m/ # second\n\nnot a url\nhttps://a.example/x\nhttps://user@c.example/y\n").unwrap();
    let defaults = ["https://c.example/d", "https://a.example/x"];
    let m = mirrors(Some("https://a.example/x/, ftp://no"), Some(&file), &defaults);
    // the environment's, then the file's, then the defaults; a repeated one is kept once
    assert_eq!(m, ["https://a.example/x", "https://b.example/m", "https://c.example/d"]);
    assert_eq!(mirrors(None, Some(Path::new("/nonexistent/mirrors.txt")), &defaults).len(), 2);
    assert!(mirrors(None, None, &[]).is_empty());
    let _ = std::fs::remove_file(&file);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Upstream LightCraft 4b57d5d: a bearer token goes only to https and only when the caller names
/// its variable (li-ai keeps it to the first host, never a redirect's).
#[test]
fn j08_a_token_is_sent_only_over_https() {
    assert_eq!(bearer(Some(" hf_abc \n"), "https://huggingface.co/x/model.bin"), Some("hf_abc"));
    assert_eq!(bearer(Some("hf_abc"), "http://huggingface.co/x"), None);
    assert_eq!(bearer(None, "https://huggingface.co/x"), None);
    assert_eq!(bearer(Some("  "), "https://huggingface.co/x"), None);
    // a caller that names no variable sends nothing, whatever the environment holds
    assert!(Options::default().token_env.is_none());
}
