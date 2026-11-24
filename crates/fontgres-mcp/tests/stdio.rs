//! Exercise the binary through the pipes used by real MCP clients.

use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};

/// Writes `messages` as JSON lines, then reads and parses one response line.
#[expect(clippy::unwrap_used, reason = "unwrap is fine in test helpers")]
async fn exchange(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
    messages: &[Value],
) -> Value {
    for message in messages {
        stdin
            .write_all(format!("{message}\n").as_bytes())
            .await
            .unwrap();
    }
    let mut line = String::new();
    stdout.read_line(&mut line).await.unwrap();
    serde_json::from_str(&line).unwrap()
}

/// Starts the server on a config that loads only the test font.
#[expect(clippy::unwrap_used, reason = "unwrap is fine in test helpers")]
fn spawn_server(directory: &tempfile::TempDir) -> tokio::process::Child {
    let config = directory.path().join("config.toml");
    let fonts =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../fontgres-core/tests/fonts");
    std::fs::write(
        &config,
        format!(
            "[fonts]\nsystem = false\ndirs = [{:?}]\n",
            fonts.display().to_string()
        ),
    )
    .unwrap();
    Command::new(assert_cmd::cargo::cargo_bin("fontgres-mcp"))
        .env_clear()
        .arg("--config")
        .arg(config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap()
}

#[tokio::test]
async fn stdio_protocol_exchange_and_eof_exit() {
    // Covers startup, the exchange, and shutdown; kill_on_drop also cleans up
    // the subprocess if a protocol assertion fails or the deadline expires.
    tokio::time::timeout(Duration::from_secs(10), async {
        let directory = tempfile::tempdir().unwrap();
        let mut child = spawn_server(&directory);
        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());

        let initialize = json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": {"name": "stdio-test", "version": "0"}
            }
        });
        let initialized_response = exchange(&mut stdin, &mut stdout, &[initialize]).await;
        assert_eq!(initialized_response["id"], 1);
        assert_eq!(
            initialized_response["result"]["protocolVersion"],
            "2025-11-25"
        );
        assert!(initialized_response["result"]["capabilities"]["tools"].is_object());

        let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        let list = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"});
        let list_response = exchange(&mut stdin, &mut stdout, &[initialized, list]).await;
        assert_eq!(list_response["id"], 2);
        let tools = list_response["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 3);
        assert!(
            tools
                .iter()
                .any(|tool| tool["name"] == "fontgres_query_font")
        );

        let call = json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "fontgres_list_families", "arguments": {}}
        });
        let call_response = exchange(&mut stdin, &mut stdout, &[call]).await;
        assert_eq!(call_response["id"], 3);
        assert_eq!(
            call_response["result"]["structuredContent"],
            json!({"items": ["Tuffy"], "total": 1, "next_offset": null})
        );

        drop(stdin);
        let mut remaining = String::new();
        stdout.read_to_string(&mut remaining).await.unwrap();
        assert!(remaining.is_empty(), "unexpected stdout: {remaining}");
        let output = child.wait_with_output().await.unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    })
    .await
    .expect("stdio exchange and EOF shutdown must finish within the deadline");
}
