//! End-to-end proof of the contract: HTTP -> registry -> SQLite -> JSON.
//!
//! This is the protocol layer the phone UI will speak, exercised without any
//! Android involvement, so a failure here means the contract broke rather than
//! the mobile integration.

use serde_json::{json, Value};
use vrcx_0_command_server::serve_loopback;
use vrcx_0_mobile_host::{build_bus, MobileRuntimeHostState};

/// Scratch directory inside the workspace.
///
/// `std::env::temp_dir()` is unusable on this build machine: the workspace has a
/// Low integrity label while %TEMP% does not, so file creation there is denied.
fn scratch_dir(label: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-tmp")
        .join(format!("http-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

async fn post_invoke(address: std::net::SocketAddr, payload: Value) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let body = payload.to_string();
    let mut stream = tokio::net::TcpStream::connect(address)
        .await
        .expect("connect");
    let request = format!(
        "POST /api/invoke HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.expect("write");
    let mut response = String::new();
    stream.read_to_string(&mut response).await.expect("read");
    response
}

#[tokio::test]
async fn serves_a_real_command_over_loopback_http() {
    let dir = scratch_dir("feed");
    let host = MobileRuntimeHostState::open(&dir).expect("open host");
    let (address, handle) = serve_loopback(build_bus(host)).await.expect("serve");

    // Empty database: the command must still answer, with an empty row list.
    let response = post_invoke(
        address,
        json!({
            "cmd": "app__feed_rows_query",
            "args": {
                "userId": "usr_test",
                "mode": "lookup",
                "maxEntries": 10
            }
        }),
    )
    .await;

    assert!(response.contains("\"ok\":true"), "unexpected body: {response}");
    assert!(
        response.contains("\"result\":[]"),
        "expected an empty row list, got: {response}"
    );

    handle.abort();
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn unknown_commands_report_the_unsupported_code() {
    let dir = scratch_dir("unsupported");
    let host = MobileRuntimeHostState::open(&dir).expect("open host");
    let (address, handle) = serve_loopback(build_bus(host)).await.expect("serve");

    let response = post_invoke(address, json!({ "cmd": "app__nope", "args": {} })).await;

    assert!(response.contains("\"ok\":false"), "unexpected body: {response}");
    assert!(response.contains("unsupported"), "unexpected body: {response}");

    handle.abort();
    let _ = std::fs::remove_dir_all(dir);
}

