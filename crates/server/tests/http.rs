//! HTTP end to end: a real server on a random port with a copy of the demo
//! contest; contestants, jury and anonymous clients through reqwest.
#![cfg(windows)]

use std::path::Path;
use std::time::{Duration, Instant};

use judge_core::model::ContestFormat;
use judge_core::AppSettings;
use online::{Account, OnlineContest, Role};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let (p, d) = (e.path(), to.join(e.file_name()));
        if p.is_dir() {
            copy_dir(&p, &d)
        } else {
            std::fs::copy(&p, &d).unwrap();
        }
    }
}

fn client() -> Client {
    let mut h = reqwest::header::HeaderMap::new();
    h.insert("x-requested-with", "oij".parse().unwrap());
    Client::builder().cookie_store(true).default_headers(h).build().unwrap()
}

async fn login(base: &str, user: &str, pw: &str) -> Client {
    let c = client();
    let r = c.post(format!("{base}/api/login")).json(&json!({ "username": user, "password": pw })).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::OK, "login {user}");
    c
}

async fn wait_judged(c: &Client, base: &str, id: i64) -> Value {
    let t0 = Instant::now();
    loop {
        let v: Value = c.get(format!("{base}/api/submissions/{id}")).send().await.unwrap().json().await.unwrap();
        if v["state"] == "done" {
            return v;
        }
        assert!(t0.elapsed() < Duration::from_secs(120), "not judged: {v}");
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_end_to_end() {
    sandbox::init_process();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/demo-contest");
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("demo");
    copy_dir(&src, &root);
    let _ = std::fs::remove_dir_all(root.join(".oij"));
    std::fs::write(root.join("data/sum/sample1.in"), "1 2\n").unwrap();

    let mut app = AppSettings::default();
    app.workers = 2;
    let (tx, notify) = server::channel();
    let contest = OnlineContest::open(&root, app, notify).unwrap();
    let mut cfg = contest.config();
    cfg.format = ContestFormat::Icpc;
    cfg.start = online::clock::format_local(online::now_ms() + 3_600_000); // not started yet
    cfg.submit_interval_s = 0;
    contest.set_config(cfg.clone()).unwrap();
    let admin_pw = contest.ensure_admin().unwrap().unwrap();
    let mk = |n: &str| Account {
        id: 0,
        username: n.into(),
        display_name: n.to_uppercase(),
        group_name: String::new(),
        seat: "A1".into(),
        role: Role::Contestant,
        ip_lock: String::new(),
        disabled: false,
    };
    contest.create_account(mk("alice"), Some("pa".into())).unwrap();
    contest.create_account(mk("bob"), Some("pb".into())).unwrap();
    contest.start_judging().unwrap();
    let state = server::AppState::new(contest.clone(), tx);
    let handle = server::start(state, "127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", handle.addr);

    // Anonymous: info without problems; public scoreboard allowed.
    let anon = client();
    let info: Value = anon.get(format!("{base}/api/info")).send().await.unwrap().json().await.unwrap();
    assert_eq!(info["phase"], "before");
    assert_eq!(info["problems"].as_array().unwrap().len(), 0);
    assert_eq!(anon.get(format!("{base}/api/scoreboard")).send().await.unwrap().status(), StatusCode::OK);
    assert_eq!(anon.get(format!("{base}/api/submissions")).send().await.unwrap().status(), StatusCode::UNAUTHORIZED);

    // Wrong password; requests without the CSRF header are refused.
    let r = anon.post(format!("{base}/api/login")).json(&json!({"username": "alice", "password": "x"})).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    let r = Client::new().post(format!("{base}/api/login")).json(&json!({"username": "alice", "password": "pa"})).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::FORBIDDEN);

    let alice = login(&base, "alice", "pa").await;
    let bob = login(&base, "bob", "pb").await;
    let admin = login(&base, "admin", &admin_pw).await;

    // Before the start: no problems, no submissions.
    let info: Value = alice.get(format!("{base}/api/info")).send().await.unwrap().json().await.unwrap();
    assert_eq!(info["problems"].as_array().unwrap().len(), 0);
    let src_ok = std::fs::read_to_string(src.join("source/alice/sum/sum.cpp")).unwrap();
    let r = alice.post(format!("{base}/api/submit")).json(&json!({"problem": "sum", "lang": "cpp", "source": src_ok})).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    assert_eq!(alice.get(format!("{base}/api/problems/sum/samples/sample1.in")).send().await.unwrap().status(), StatusCode::FORBIDDEN);

    // Start the contest (admin) and listen to live events as alice.
    cfg.start = online::clock::format_local(online::now_ms() - 60_000);
    let r = admin.put(format!("{base}/api/jury/config")).json(&cfg).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::NO_CONTENT);
    assert_eq!(alice.put(format!("{base}/api/jury/config")).json(&cfg).send().await.unwrap().status(), StatusCode::FORBIDDEN);
    let mut sse = alice.get(format!("{base}/api/events")).send().await.unwrap();

    let info: Value = alice.get(format!("{base}/api/info")).send().await.unwrap().json().await.unwrap();
    assert_eq!(info["phase"], "running");
    let sum = info["problems"].as_array().unwrap().iter().find(|p| p["name"] == "sum").unwrap().clone();
    assert_eq!(sum["letter"], "A");
    assert_eq!(sum["samples"], json!(["sample1.in"]));
    let r = alice.get(format!("{base}/api/problems/sum/samples/sample1.in")).send().await.unwrap();
    assert_eq!(r.text().await.unwrap(), "1 2\n");
    // Test data is never served.
    for path in ["samples/sum1.in", "samples/..%2Fsum%2Fsum1.in", "files/sum1.in", "files/..%2F..%2Fcontest.toml", "statement"] {
        let s = alice.get(format!("{base}/api/problems/sum/{path}")).send().await.unwrap().status();
        assert_eq!(s, StatusCode::NOT_FOUND, "{path}");
    }

    let r: Value = alice.post(format!("{base}/api/submit")).json(&json!({"problem": "sum", "lang": "cpp", "source": src_ok})).send().await.unwrap().json().await.unwrap();
    let id = r["id"].as_i64().unwrap();
    let v = wait_judged(&alice, &base, id).await;
    assert_eq!(v["label"], "AC", "{v}");
    assert!(v["source"].as_str().unwrap().contains("main"));
    assert_eq!(v["score"], Value::Null); // ICPC: no points shown to contestants

    // The SSE stream told alice about her submission.
    let t0 = Instant::now();
    let mut seen = String::new();
    while !seen.contains("\"submission\"") && t0.elapsed() < Duration::from_secs(10) {
        if let Ok(Ok(Some(chunk))) = tokio::time::timeout(Duration::from_secs(5), sse.chunk()).await {
            seen.push_str(&String::from_utf8_lossy(&chunk));
        }
    }
    assert!(seen.contains("\"submission\""), "{seen}");

    // Others' submissions and jury endpoints are hidden from contestants.
    assert_eq!(bob.get(format!("{base}/api/submissions/{id}")).send().await.unwrap().status(), StatusCode::NOT_FOUND);
    let bobs: Value = bob.get(format!("{base}/api/submissions")).send().await.unwrap().json().await.unwrap();
    assert_eq!(bobs.as_array().unwrap().len(), 0);
    for (m, p) in [("GET", "/api/jury/accounts"), ("GET", "/api/jury/status"), ("POST", "/api/jury/rejudge"), ("GET", "/api/balloons"), ("GET", "/api/print/queue")] {
        let req = if m == "GET" { bob.get(format!("{base}{p}")) } else { bob.post(format!("{base}{p}")).json(&json!({"ids": [id]})) };
        assert_eq!(req.send().await.unwrap().status(), StatusCode::FORBIDDEN, "{p}");
    }

    // Wrong answer and compile error details.
    let r: Value = bob.post(format!("{base}/api/submit")).json(&json!({"problem": "sum", "lang": "x.c", "source": "int main( {"})).send().await.unwrap().json().await.unwrap();
    let v = wait_judged(&bob, &base, r["id"].as_i64().unwrap()).await;
    assert_eq!(v["label"], "CE");
    assert!(!v["message"].as_str().unwrap().is_empty(), "compiler output shown for CE");
    let r = bob.post(format!("{base}/api/submit")).json(&json!({"problem": "sum", "lang": "cpp", "source": "x".repeat(70 * 1024)})).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    let e: Value = r.json().await.unwrap();
    assert_eq!(e["detail"]["reason"], "too_large");

    // Scoreboard, balloons, jury view of everything.
    let board: Value = anon.get(format!("{base}/api/scoreboard")).send().await.unwrap().json().await.unwrap();
    assert_eq!(board["rows"][0]["username"], "alice");
    assert_eq!(board["rows"][0]["solved"], 1);
    let balloons: Value = admin.get(format!("{base}/api/balloons")).send().await.unwrap().json().await.unwrap();
    assert_eq!(balloons[0]["letter"], "A");
    assert_eq!(balloons[0]["first"], true);
    let all: Value = admin.get(format!("{base}/api/submissions")).send().await.unwrap().json().await.unwrap();
    assert_eq!(all.as_array().unwrap().len(), 2);
    let detail: Value = admin.get(format!("{base}/api/submissions/{id}")).send().await.unwrap().json().await.unwrap();
    assert!(!detail["subtasks"].as_array().unwrap().is_empty());

    // Clarification round trip; print job.
    let r = alice.post(format!("{base}/api/clarifications")).json(&json!({"problem": "sum", "question": "Is n > 0?"})).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let qid = r.json::<Value>().await.unwrap()["id"].as_i64().unwrap();
    let r = admin.post(format!("{base}/api/jury/clarifications/{qid}/answer")).json(&json!({"answer": "Yes.", "public": true})).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::NO_CONTENT);
    let bq: Value = bob.get(format!("{base}/api/clarifications")).send().await.unwrap().json().await.unwrap();
    assert_eq!(bq[0]["answer"], "Yes.");
    assert_eq!(alice.post(format!("{base}/api/print")).json(&json!({"filename": "a.cpp", "content": "int x;"})).send().await.unwrap().status(), StatusCode::OK);
    let q: Value = admin.get(format!("{base}/api/print/queue")).send().await.unwrap().json().await.unwrap();
    assert_eq!(q[0]["seat"], "A1");

    // Account import returns generated passwords that work.
    let creds: Value = admin
        .post(format!("{base}/api/jury/accounts/import"))
        .json(&json!({"csv": "username,name,school,seat\ncarol,Carol,S,B2\n"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let pw = creds[0]["password"].as_str().unwrap();
    login(&base, "carol", pw).await;

    // Static pages: unknown paths fall back to the single-page app (or a hint).
    let r = anon.get(format!("{base}/scoreboard")).send().await.unwrap();
    assert!(r.status() == StatusCode::OK || r.status() == StatusCode::NOT_FOUND);
    assert_eq!(anon.get(format!("{base}/api/nope")).send().await.unwrap().status(), StatusCode::NOT_FOUND);

    handle.stop().await;
    contest.stop_judging();
}
