use super::*;

#[test]
fn review_unload_refuses_http_errors_and_keeps_busy_queue_untouched() {
    for status in [200, 404, 503] {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let host = server.server_addr().to_string();
        let worker = std::thread::spawn(move || {
            for path in ["/queue", "/free"] {
                let request = server.recv_timeout(Duration::from_secs(3)).unwrap().unwrap();
                assert_eq!(request.url(), path);
                let (code, body) =
                    if path == "/queue" { (200, json!({"queue_running":[],"queue_pending":[]})) } else { (status, json!({"error":"synthetic refusal"})) };
                request.respond(tiny_http::Response::from_string(body.to_string()).with_status_code(code)).unwrap();
            }
        });
        let result = ComfyClient::new(host).free_memory();
        assert_eq!(result.is_ok(), status == 200);
        if status != 200 {
            assert!(result.unwrap_err().to_string().contains(&status.to_string()));
        }
        worker.join().unwrap();
    }
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let host = server.server_addr().to_string();
    let worker = std::thread::spawn(move || {
        let request = server.recv_timeout(Duration::from_secs(3)).unwrap().unwrap();
        assert_eq!(request.url(), "/queue");
        request.respond(tiny_http::Response::from_string(json!({"queue_running":[[0,"someone-else"]],"queue_pending":[]}).to_string())).unwrap();
        assert!(server.recv_timeout(Duration::from_millis(100)).unwrap().is_none());
    });
    assert!(ComfyClient::new(host).free_memory().unwrap_err().to_string().contains("busy"));
    worker.join().unwrap();
}

#[test]
fn review_failed_polling_and_deadline_retain_and_reconcile_owned_prompts() {
    for polling_failure in [true, false] {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let host = server.server_addr().to_string();
        let restored = Arc::new(AtomicBool::new(false));
        let ready = restored.clone();
        let running = Arc::new(AtomicBool::new(true));
        let live = running.clone();
        let done = Arc::new(AtomicBool::new(false));
        let stop = done.clone();
        let paths = Arc::new(Mutex::new(Vec::new()));
        let seen = paths.clone();
        let worker = std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                let Some(request) = server.recv_timeout(Duration::from_millis(20)).unwrap() else { continue };
                let path = request.url().to_owned();
                seen.lock().unwrap().push(path.clone());
                let (status, value) = match path.as_str() {
                    "/prompt" => (200, json!({"prompt_id":"ours"})),
                    "/history/ours" if polling_failure && !ready.load(Ordering::SeqCst) => (503, json!({})),
                    "/history/ours" => (200, json!({})),
                    "/queue" => {
                        let mut queue = vec![json!([1, "unrelated"])];
                        if live.load(Ordering::SeqCst) {
                            queue.push(json!([0, "ours"]));
                        }
                        (200, json!({"queue_running":queue,"queue_pending":[]}))
                    }
                    "/api/jobs/ours/cancel" if ready.load(Ordering::SeqCst) => {
                        live.store(false, Ordering::SeqCst);
                        (200, json!({"cancelled":true}))
                    }
                    "/api/jobs/ours/cancel" => (503, json!({})),
                    _ => (404, json!({})),
                };
                let _ = request.respond(tiny_http::Response::from_string(value.to_string()).with_status_code(status));
            }
        });
        let client = ComfyClient::new(&host);
        let ctl = JobControl::new();
        let short = Duration::from_millis(30);
        let long = Duration::from_secs(5);
        let result =
            client.run_bounded(json!({}), &[], &ctl, false, None, if polling_failure { long } else { short }, if polling_failure { short } else { long });
        assert!(result.is_err());
        assert!(running.load(Ordering::SeqCst));
        assert!(our_prompts(&host).contains(&"ours".to_owned()), "failed jobs keep remote ownership");
        restored.store(true, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(3);
        while our_prompts(&host).contains(&"ours".to_owned()) {
            assert!(Instant::now() < deadline, "reconnected job did not reconcile");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!running.load(Ordering::SeqCst));
        done.store(true, Ordering::SeqCst);
        worker.join().unwrap();
        let paths = paths.lock().unwrap();
        assert!(paths.iter().any(|p| p == "/api/jobs/ours/cancel"));
        assert!(!paths.iter().any(|p| p == "/interrupt" || p.starts_with("/view") || p.contains("unrelated")), "{paths:?}");
    }
}

#[test]
fn review_absent_then_listed_and_malformed_queues_never_disown_live_prompt() {
    for scenario in 0..3 {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let host = server.server_addr().to_string();
        let restored = Arc::new(AtomicBool::new(false));
        let ready = restored.clone();
        let running = Arc::new(AtomicBool::new(true));
        let live = running.clone();
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = stopped.clone();
        let worker = std::thread::spawn(move || {
            let mut queues = 0;
            while !stop.load(Ordering::SeqCst) {
                let Some(request) = server.recv_timeout(Duration::from_millis(20)).unwrap() else { continue };
                let (status, value) = match request.url() {
                    "/prompt" => (200, json!({"prompt_id":"ours"})),
                    "/history/ours" => (200, json!({})),
                    "/queue" => {
                        queues += 1;
                        let body = if !ready.load(Ordering::SeqCst) && scenario == 1 {
                            json!({})
                        } else if !ready.load(Ordering::SeqCst) && scenario == 2 {
                            json!({"queue_running":"invalid","queue_pending":[]})
                        } else {
                            let visible = live.load(Ordering::SeqCst) && !(scenario == 0 && queues == 1);
                            json!({"queue_running":if visible {vec![json!([0,"ours"])]} else {vec![]},"queue_pending":[]})
                        };
                        (200, body)
                    }
                    "/api/jobs/ours/cancel" if ready.load(Ordering::SeqCst) => {
                        live.store(false, Ordering::SeqCst);
                        (200, json!({"cancelled":true}))
                    }
                    "/api/jobs/ours/cancel" => (503, json!({})),
                    _ => (404, json!({})),
                };
                let _ = request.respond(tiny_http::Response::from_string(value.to_string()).with_status_code(status));
            }
        });
        let result = ComfyClient::new(&host).run_bounded(
            json!({}),
            &[],
            &JobControl::new(),
            false,
            None,
            if scenario == 0 { Duration::from_millis(1300) } else { Duration::from_secs(5) },
            if scenario == 0 { Duration::from_secs(5) } else { Duration::from_millis(30) },
        );
        assert!(result.is_err());
        assert!(running.load(Ordering::SeqCst));
        assert!(our_prompts(&host).contains(&"ours".to_owned()), "scenario {scenario} lost a live prompt");
        restored.store(true, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(3);
        while our_prompts(&host).contains(&"ours".to_owned()) {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!running.load(Ordering::SeqCst));
        stopped.store(true, Ordering::SeqCst);
        worker.join().unwrap();
    }
}

#[test]
fn review_accepted_submission_with_lost_reply_reconciles_unique_client_without_reposting() {
    for legacy in [false, true] {
        for completed in [false, true] {
            let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
            let host = server.server_addr().to_string();
            let server_host = host.clone();
            let restored = Arc::new(AtomicBool::new(false));
            let ready = restored.clone();
            let running = Arc::new(AtomicBool::new(!completed));
            let live = running.clone();
            let stopped = Arc::new(AtomicBool::new(false));
            let stop = stopped.clone();
            let posts = Arc::new(AtomicUsize::new(0));
            let posted = posts.clone();
            let scans = Arc::new(AtomicUsize::new(0));
            let scanned = scans.clone();
            let paths = Arc::new(Mutex::new(Vec::new()));
            let seen = paths.clone();
            let worker = std::thread::spawn(move || {
                let (mut client_id, mut id) = (String::new(), String::new());
                while !stop.load(Ordering::SeqCst) {
                    let Some(mut request) = server.recv_timeout(Duration::from_millis(20)).unwrap() else { continue };
                    let path = request.url().to_owned();
                    let method = request.method().as_str().to_owned();
                    seen.lock().unwrap().push(path.clone());
                    let mut body = String::new();
                    request.as_reader().read_to_string(&mut body).unwrap();
                    let value: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                    if path == "/prompt" {
                        posted.fetch_add(1, Ordering::SeqCst);
                        client_id = value["client_id"].as_str().unwrap().into();
                        assert_eq!(value["prompt_id"], client_id);
                        assert!(uuid::Uuid::parse_str(&client_id).is_ok());
                        assert!(OURS.lock().unwrap().iter().any(|p| p.host == server_host && p.client_id == client_id && p.id.is_none()));
                        id = if legacy { "server-minted-id".into() } else { client_id.clone() };
                        if legacy {
                            // The server accepted the prompt, but the transport has no usable reply.
                            drop(request);
                        } else {
                            request.respond(tiny_http::Response::from_string("malformed reply")).unwrap();
                        }
                        continue;
                    }
                    let history_entry = json!({"prompt":[0,id,{}, {"client_id":client_id}, []], "status":{"completed":true},"outputs":{}});
                    let (status, response) = if !ready.load(Ordering::SeqCst) && path == "/queue" {
                        scanned.fetch_add(1, Ordering::SeqCst);
                        (200, json!({"queue_running":[],"queue_pending":[]}))
                    } else if !ready.load(Ordering::SeqCst) && path.starts_with("/history") {
                        (200, json!({}))
                    } else if path == "/queue" && method == "GET" {
                        let ours = json!([0,id,{}, {"client_id":client_id}, []]);
                        (
                            200,
                            json!({"queue_running":[[1,"unrelated",{}, {"client_id":"another-client"}, []]],
                            "queue_pending":if live.load(Ordering::SeqCst) {vec![ours]} else {vec![]}}),
                        )
                    } else if path == "/queue" && method == "POST" {
                        assert_eq!(value, json!({"delete":[id]}));
                        assert!(OURS.lock().unwrap().iter().any(|p| p.host == server_host && p.client_id == client_id && p.vram.is_some()));
                        live.store(false, Ordering::SeqCst);
                        (200, json!({}))
                    } else if path == "/history?max_items=64" || path == format!("/history/{id}") {
                        (200, if completed { json!({id.clone():history_entry}) } else { json!({}) })
                    } else if path == format!("/api/jobs/{id}/cancel") && !legacy {
                        assert!(OURS.lock().unwrap().iter().any(|p| p.host == server_host && p.client_id == client_id && p.vram.is_some()));
                        live.store(false, Ordering::SeqCst);
                        (200, json!({"cancelled":true}))
                    } else {
                        (404, json!({}))
                    };
                    let _ = request.respond(tiny_http::Response::from_string(response.to_string()).with_status_code(status));
                }
            });
            let result = ComfyClient::new(&host).run(json!({}), &[], &JobControl::new());
            assert!(result.is_err());
            let deadline = Instant::now() + Duration::from_secs(3);
            while !OURS.lock().unwrap().iter().any(|p| p.host == host && p.unresolved && p.id.is_none() && p.vram.is_none()) {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(scans.load(Ordering::SeqCst) >= 3, "confirmed idle observations release pressure without dropping identity");
            assert!(OURS.lock().unwrap().iter().any(|p| p.host == host && p.unresolved && p.id.is_none()));
            restored.store(true, Ordering::SeqCst);
            let deadline = Instant::now() + Duration::from_secs(3);
            while OURS.lock().unwrap().iter().any(|p| p.host == host) {
                assert!(Instant::now() < deadline, "accepted submission did not reconcile");
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(!running.load(Ordering::SeqCst));
            assert_eq!(posts.load(Ordering::SeqCst), 1, "never blindly retry a possibly accepted POST");
            stopped.store(true, Ordering::SeqCst);
            worker.join().unwrap();
            assert!(!paths.lock().unwrap().iter().any(|p| p == "/interrupt" || p.starts_with("/view") || p.contains("unrelated")));
        }
    }
}

#[test]
fn review_explicit_prompt_rejection_releases_submission_lease() {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let host = server.server_addr().to_string();
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = stopped.clone();
    let worker = std::thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            let Some(request) = server.recv_timeout(Duration::from_millis(20)).unwrap() else { continue };
            let response = if request.url() == "/prompt" {
                tiny_http::Response::from_string(json!({"error":{"message":"invalid synthetic workflow"}}).to_string()).with_status_code(400)
            } else {
                tiny_http::Response::from_string("{}").with_status_code(404)
            };
            let _ = request.respond(response);
        }
    });
    assert!(ComfyClient::new(&host).run(json!({}), &[], &JobControl::new()).unwrap_err().is::<PromptRejected>());
    assert!(!OURS.lock().unwrap().iter().any(|p| p.host == host));
    stopped.store(true, Ordering::SeqCst);
    worker.join().unwrap();
}

#[test]
fn review_pre_send_failures_release_ownership_but_response_failures_remain_uncertain() {
    assert!(submission_not_sent(&ureq::Error::HostNotFound));
    assert!(submission_not_sent(&ureq::Error::ConnectionFailed));
    assert!(submission_not_sent(&ureq::Error::Timeout(ureq::Timeout::Connect)));
    assert!(submission_not_sent(&ureq::Error::Timeout(ureq::Timeout::Resolve)));
    assert!(!submission_not_sent(&ureq::Error::Timeout(ureq::Timeout::Global)));
    assert!(!submission_not_sent(&ureq::Error::Timeout(ureq::Timeout::RecvResponse)));
    assert!(!submission_not_sent(&ureq::Error::Io(std::io::ErrorKind::ConnectionReset.into())));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let host = listener.local_addr().unwrap().to_string();
    drop(listener);
    let error = ComfyClient::new(&host).run(json!({}), &[], &JobControl::new()).unwrap_err();
    assert!(error.is::<SubmissionNotSent>(), "{error:#}");
    assert!(!OURS.lock().unwrap().iter().any(|p| p.host == host));
}

#[test]
fn review_unknown_submission_pressure_recovers_without_discarding_identity() {
    let mut prompt = OwnedPrompt {
        host: "127.0.0.1:1".into(),
        client_id: "unique-client".into(),
        id: None,
        control: JobControl::new(),
        unresolved: true,
        _permit: PromptPermit::reserve().unwrap(),
        local: true,
        vram: Some(lightcraft_sysmem::gpu::AiJob::new(true)),
        idle_observations: 0,
    };
    let idle = SubmissionObservation { id: None, busy: false };
    for _ in 0..2 {
        update_submission_pressure(&mut prompt, Some(&idle));
        assert!(prompt.vram.is_some());
    }
    update_submission_pressure(&mut prompt, None); // Failed observation cannot count as idle.
    for _ in 0..3 {
        update_submission_pressure(&mut prompt, Some(&idle));
    }
    assert!(prompt.vram.is_none());
    assert!(prompt.unresolved && prompt.id.is_none());
    assert_eq!(prompt.client_id, "unique-client");
    update_submission_pressure(&mut prompt, Some(&SubmissionObservation { id: None, busy: true }));
    assert!(prompt.vram.is_some(), "later queue activity restores local pressure");
    for _ in 0..3 {
        update_submission_pressure(&mut prompt, Some(&idle));
    }
    assert!(prompt.vram.is_none());
    update_submission_pressure(&mut prompt, None);
    assert!(prompt.vram.is_some(), "loss of current idle proof restores conservative pressure");
}
