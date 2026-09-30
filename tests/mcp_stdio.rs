//! `plan-my-cabinet --mcp` speaks MCP over stdio: handshake, tool listing, a
//! small build with a picture, a save, and a clean exit when stdin closes.
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

struct Client {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: mpsc::Receiver<String>,
    next: u64,
}

impl Client {
    fn start(user_data: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_plan-my-cabinet"))
            .args(["--mcp", "--user-data-dir"])
            .arg(user_data)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("server starts");
        let stdin = child.stdin.take();
        let stdout: ChildStdout = child.stdout.take().expect("piped stdout");
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin,
            lines,
            next: 0,
        }
    }

    fn write(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().expect("open stdin");
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        self.write(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let line = self
                .lines
                .recv_timeout(Duration::from_secs(120))
                .expect("server answers in time");
            let message: Value = serde_json::from_str(&line).expect("stdout carries only JSON-RPC");
            if message["id"] == id {
                assert!(message.get("error").is_none(), "{method}: {message}");
                return message["result"].clone();
            }
        }
    }

    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        let result = self.request(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        );
        assert_ne!(result["isError"], true, "{tool}: {result}");
        result
    }

    fn json(result: &Value) -> Value {
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
    }
}

#[test]
fn stdio_server_builds_renders_and_saves_a_cabinet() {
    let dir = std::env::temp_dir().join(format!("pmcab-mcp-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut client = Client::start(&dir);

    let init = client.request(
        "initialize",
        json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }),
    );
    assert_eq!(init["serverInfo"]["name"], "plan-my-cabinet");
    assert!(
        init["instructions"]
            .as_str()
            .unwrap()
            .contains("describe_scene")
    );
    client.write(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));

    let tools = client.request("tools/list", json!({}));
    let names: Vec<&str> = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for expected in [
        "generate_template",
        "create_boards",
        "place_board_on_face",
        "describe_scene",
        "render_views",
        "add_needed_sheets",
        "optimize_cut_plan",
        "add_hinges",
        "create_door",
        "save_project",
    ] {
        assert!(names.contains(&expected), "missing {expected}");
    }

    client.call(
        "generate_template",
        json!({ "kind": "wall", "name": "Wall 600" }),
    );
    let scene = Client::json(&client.call("describe_scene", json!({})));
    assert_eq!(scene["overlaps"], json!([]));

    let picture = client.call(
        "render_view",
        json!({ "view": "front", "projection": "orthographic", "width": 320, "height": 240 }),
    );
    let image = picture["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "image")
        .expect("an image block");
    assert_eq!(image["mimeType"], "image/png");
    assert!(!image["data"].as_str().unwrap().is_empty());

    // Errors are tool results the agent can read, not protocol failures.
    let missing = client.request(
        "tools/call",
        json!({ "name": "get_board", "arguments": { "ref": "nope" } }),
    );
    assert_eq!(missing["isError"], true);
    let error: Value =
        serde_json::from_str(missing["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(error["code"], "not_found");

    let path = dir.join("wall.pmcab");
    let saved = Client::json(&client.call("save_project", json!({ "path": path })));
    assert_eq!(saved["saved"], true);
    let bytes = std::fs::read(&path).unwrap();
    plan_my_cabinet::persistence::prepare_bytes(&bytes).expect("a valid project file");

    // Closing stdin ends the session.
    drop(client.stdin.take());
    let status = client.child.wait().unwrap();
    assert!(status.success());
    std::fs::remove_dir_all(&dir).ok();
}
