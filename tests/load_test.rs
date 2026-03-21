use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Load test: fire 1000 concurrent requests at the SSE server and verify
/// all complete successfully within a reasonable time.
#[tokio::test]
async fn test_sse_handles_1000_concurrent_requests() {
    let tmp = TempDir::new().unwrap();
    let fast_tool = fixture_path("fast_tool.sh");

    // Register tool
    let status = std::process::Command::new(assert_cmd::cargo::cargo_bin("mcpw"))
        .args([
            "register",
            "fast_tool",
            "--cmd",
            fast_tool.to_str().unwrap(),
            "--desc",
            "Fast tool for load testing",
            "--type",
            "sse",
            "--force",
        ])
        .env("MCPW_TOOLS_DIR", tmp.path())
        .output()
        .expect("Failed to register");
    assert!(status.status.success());

    // Start SSE server on a random port
    let port = 19000 + (std::process::id() % 1000) as u16;
    let bin_path = assert_cmd::cargo::cargo_bin("mcpw");
    let mut server = std::process::Command::new(&bin_path)
        .args(["serve", "--port", &port.to_string()])
        .env("MCPW_TOOLS_DIR", tmp.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to start SSE server");

    // Wait for server to be ready
    let base_url = format!("http://127.0.0.1:{}", port);
    let client = reqwest::Client::new();
    let mut ready = false;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if client
            .post(format!("{}/mcp", base_url))
            .json(&serde_json::json!({"jsonrpc":"2.0","id":0,"method":"ping"}))
            .send()
            .await
            .is_ok()
        {
            ready = true;
            break;
        }
    }
    assert!(ready, "SSE server did not start within 5 seconds");

    // Initialize
    let init_resp = client
        .post(format!("{}/mcp", base_url))
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        }))
        .send()
        .await
        .expect("init failed");
    assert!(init_resp.status().is_success());

    // Fire 1000 concurrent tool calls
    let total_requests: u32 = 1000;
    let success_count = Arc::new(AtomicU32::new(0));
    let error_count = Arc::new(AtomicU32::new(0));

    let start = Instant::now();

    let mut handles = Vec::new();
    for i in 0..total_requests {
        let client = client.clone();
        let url = format!("{}/mcp", base_url);
        let success = Arc::clone(&success_count);
        let errors = Arc::clone(&error_count);

        handles.push(tokio::spawn(async move {
            let resp = client
                .post(&url)
                .json(&serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": i + 100,
                    "method": "tools/call",
                    "params": {
                        "name": "fast_tool",
                        "arguments": {}
                    }
                }))
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    let body: serde_json::Value = r.json().await.unwrap_or_default();
                    if body["result"]["isError"] == false {
                        success.fetch_add(1, Ordering::Relaxed);
                    } else {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
                _ => {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    // Wait for all requests to complete
    for handle in handles {
        let _ = handle.await;
    }

    let elapsed = start.elapsed();
    let successes = success_count.load(Ordering::Relaxed);
    let errors = error_count.load(Ordering::Relaxed);

    // Clean up
    let _ = server.kill();
    let _ = server.wait();

    // Report results
    eprintln!(
        "Load test: {} requests, {} successes, {} errors, {:.2}s elapsed, {:.0} req/s",
        total_requests,
        successes,
        errors,
        elapsed.as_secs_f64(),
        total_requests as f64 / elapsed.as_secs_f64()
    );

    // Assertions
    assert!(
        successes >= 950, // Allow up to 5% failure under load
        "Expected at least 950 successes, got {} (errors: {})",
        successes,
        errors
    );
    assert!(
        elapsed < Duration::from_secs(60),
        "1000 requests should complete within 60 seconds, took {:.2}s",
        elapsed.as_secs_f64()
    );
}
