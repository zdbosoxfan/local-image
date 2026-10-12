use super::*;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

fn server(handler: impl FnOnce(TcpStream, String) + Send + 'static) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    allow_test_host(&address);
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        stream.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            if stream.read(&mut byte).unwrap() == 0 {
                break;
            }
            request.push(byte[0]);
            assert!(request.len() < 16384);
        }
        handler(stream, String::from_utf8(request).unwrap());
    });
    (format!("http://{address}/model"), worker)
}
fn directory() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("li-cancel-dl-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[test]
fn stalled_download_cancellation_closes_writer_cleans_partial_and_allows_retry() {
    let (release, held) = mpsc::channel();
    let (url, server) = server(move |mut stream, _| {
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nabc").unwrap();
        let _ = held.recv_timeout(Duration::from_secs(4));
    });
    let dir = directory();
    let dest = dir.join("weights.bin");
    let ctl = JobControl::new();
    let (progress, first) = mpsc::sync_channel(1);
    let (finished, done) = mpsc::channel();
    let (control, target) = (ctl.clone(), dest.clone());
    let worker = thread::spawn(move || {
        finished
            .send(download_file(&url, &target, 10, &sha(b"abcdefghij"), &control, &|n| {
                let _ = progress.try_send(n);
            }))
            .unwrap();
    });
    assert!(first.recv_timeout(Duration::from_secs(3)).unwrap() > 0);
    // Let the streaming worker enter its next receive while the server holds the body.
    thread::sleep(Duration::from_millis(50));
    let start = Instant::now();
    ctl.cancel();
    let result = done.recv_timeout(Duration::from_secs(2)).expect("stalled receive must observe cancellation");
    assert!(result.unwrap_err().is::<Cancelled>());
    eprintln!("Stalled model download cancellation: {:.1} ms", start.elapsed().as_secs_f64() * 1000.0);
    assert!(start.elapsed() < Duration::from_secs(2));
    worker.join().unwrap();
    release.send(()).unwrap();
    server.join().unwrap();
    assert!(!dest.exists());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    let (url, server) = self::server(|mut stream, _| {
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nabcdefghij").unwrap();
    });
    download_file(&url, &dest, 10, &sha(b"abcdefghij"), &JobControl::new(), &|_| {}).unwrap();
    server.join().unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), b"abcdefghij");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cancel_at_final_progress_and_racing_destination_never_publish_or_replace() {
    for cancel in [true, false] {
        let (url, server) = server(|mut stream, _| {
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello").unwrap();
        });
        let dir = directory();
        let dest = dir.join("weights.bin");
        let ctl = JobControl::new();
        let result = download_file(&url, &dest, 5, &sha(b"hello"), &ctl, &|n| {
            if n == 5 {
                if cancel {
                    ctl.cancel();
                } else {
                    std::fs::write(&dest, b"user-existing-file").unwrap();
                }
            }
        });
        server.join().unwrap();
        if cancel {
            assert!(result.unwrap_err().is::<Cancelled>());
            assert!(!dest.exists());
        } else {
            assert!(result.is_err());
            assert_eq!(std::fs::read(&dest).unwrap(), b"user-existing-file");
        }
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), usize::from(!cancel));
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn receive_slices_allow_long_transfers_but_do_not_extend_the_body_deadline() {
    for (spacing, budget, succeeds) in [(230, 3000, true), (55, 180, false)] {
        let (url, server) = server(move |mut stream, _| {
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\n").unwrap();
            for byte in b"hello" {
                thread::sleep(Duration::from_millis(spacing));
                if stream.write_all(&[*byte]).is_err() {
                    break;
                }
            }
        });
        let ctl = JobControl::new();
        let agent = download_agent(Duration::from_millis(budget), &ctl);
        let (_, mut response) = open(&agent, &url, None, false, &ctl).unwrap();
        let mut body = Vec::new();
        let result = response.body_mut().as_reader().read_to_end(&mut body);
        if succeeds {
            result.unwrap();
            assert_eq!(body, b"hello");
        } else {
            assert!(result.is_err());
            assert!(!ctl.is_cancelled());
        }
        drop(response);
        server.join().unwrap();
    }
}

#[test]
fn stalled_headers_cancel_and_cross_authority_redirect_drops_token() {
    let (ready, started) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let (url, server) = server(move |_stream, _| {
        ready.send(()).unwrap();
        let _ = held.recv_timeout(Duration::from_secs(4));
    });
    let ctl = JobControl::new();
    let control = ctl.clone();
    let (done, result) = mpsc::channel();
    let worker = thread::spawn(move || {
        let agent = download_agent(Duration::from_secs(10), &control);
        done.send(open(&agent, &url, None, false, &control).map(|_| ())).unwrap();
    });
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    ctl.cancel();
    assert!(result.recv_timeout(Duration::from_secs(2)).unwrap().unwrap_err().is::<Cancelled>());
    worker.join().unwrap();
    release.send(()).unwrap();
    server.join().unwrap();

    let (next, target) = self::server(|mut stream, request| {
        assert!(!request.to_lowercase().contains("authorization:"));
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\nx").unwrap();
    });
    let (url, first) = self::server(move |mut stream, request| {
        assert!(request.to_lowercase().contains("authorization: bearer test-download-token"));
        write!(stream, "HTTP/1.1 302 Found\r\nLocation: {next}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    let ctl = JobControl::new();
    let agent = download_agent(Duration::from_secs(3), &ctl);
    let (_, mut response) = open(&agent, &url, Some("test-download-token"), false, &ctl).unwrap();
    assert_eq!(response.body_mut().read_to_vec().unwrap(), b"x");
    first.join().unwrap();
    target.join().unwrap();
}

#[test]
fn cancellation_interrupts_stalled_tls_handshake_below_rustls() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("https://{}/", listener.local_addr().unwrap());
    let (ready, started) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut hello = [0; 1024];
        assert!(socket.read(&mut hello).unwrap() > 0);
        ready.send(()).unwrap();
        let _ = held.recv_timeout(Duration::from_secs(4));
    });
    let ctl = JobControl::new();
    let control = ctl.clone();
    let (done, result) = mpsc::channel();
    let worker = thread::spawn(move || {
        done.send(download_agent(Duration::from_secs(10), &control).get(&url).call().is_err()).unwrap();
    });
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    ctl.cancel();
    assert!(result.recv_timeout(Duration::from_secs(2)).expect("TLS handshake must poll the raw socket"));
    worker.join().unwrap();
    release.send(()).unwrap();
    server.join().unwrap();
}

#[test]
fn failed_integrity_or_progress_unwind_leaves_no_final_or_partial_file() {
    for failure in ["checksum", "oversize", "eof", "unwind"] {
        let (url, server) = server(move |mut stream, _| {
            let body = if failure == "oversize" { "hello!" } else { "hello" };
            let length = if failure == "eof" { 10 } else { body.len() };
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{body}").unwrap();
        });
        let dir = directory();
        let dest = dir.join("weights.bin");
        let size = if failure == "eof" { 10 } else { 5 };
        let expected = if failure == "checksum" { sha(b"other") } else { sha(b"hello") };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            download_file(&url, &dest, size, &expected, &JobControl::new(), &|_| {
                assert!(failure != "unwind", "deliberate progress unwind");
            })
        }));
        if failure == "unwind" {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().is_err());
        }
        server.join().unwrap();
        assert!(!dest.exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
