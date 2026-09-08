//! Integration tests for the MCP server over stdio. Only headless tools are
//! exercised here; capture tools need a Wayland compositor.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};

struct Server {
    child: Child,
    reader: BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl Server {
    fn start(args: &[&str], home: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_omacapture"))
            .arg("mcp")
            .args(args)
            // Isolate config/data so the test never touches the developer's files.
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("XDG_STATE_HOME", home.join(".local/state"))
            .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn omacapture mcp");
        let reader = BufReader::new(child.stdout.take().unwrap());
        let mut s = Server { child, reader, next_id: 1 };
        let init = s.call(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}),
        );
        assert_eq!(init["result"]["serverInfo"]["name"], "omacapture");
        s
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let stdin = self.child.stdin.as_mut().unwrap();
        stdin.write_all(serde_json::to_string(&msg).unwrap().as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        self.reader.read_line(&mut line).unwrap();
        serde_json::from_str(&line).expect("valid JSON-RPC response")
    }

    fn tool(&mut self, name: &str, args: Value) -> (bool, String) {
        let r = self.call("tools/call", json!({"name": name, "arguments": args}));
        let res = &r["result"];
        (res["isError"].as_bool().unwrap_or(false), res["content"][0]["text"].as_str().unwrap_or("").to_string())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn scratch() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("omacapture-mcp-test-{}-{}", std::process::id(), rand_suffix()));
    std::fs::create_dir_all(dir.join("Pictures/Screenshots")).unwrap();
    dir
}

fn rand_suffix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64 % 1_000_000
}

fn blank_png(path: &std::path::Path, w: u32, h: u32) {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba([250, 250, 250, 255]));
    img.save(path).unwrap();
}

#[test]
fn lists_tools_and_resources() {
    let home = scratch();
    let mut s = Server::start(&[], &home);
    let tools = s.call("tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in [
        "capture_screen",
        "describe_screen",
        "compare",
        "annotate",
        "redact",
        "describe_annotations",
        "get_config",
        "set_config",
        "history_list",
        "read_image",
    ] {
        assert!(names.contains(&expected), "missing tool {expected}");
    }
    let res = s.call("resources/list", json!({}));
    let uris: Vec<&str> = res["result"]["resources"].as_array().unwrap().iter().map(|r| r["uri"].as_str().unwrap()).collect();
    assert!(uris.contains(&"omacapture://latest"));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn annotate_draws_every_item_type_and_respects_the_write_fence() {
    let home = scratch();
    let src = home.join("Pictures/Screenshots/src.png");
    blank_png(&src, 400, 300);
    let mut s = Server::start(&[], &home);
    let items = json!([
        {"type":"rect","x":10,"y":10,"width":100,"height":60},
        {"type":"filled_rect","x":120,"y":10,"width":60,"height":60,"color":"#00ff00"},
        {"type":"oval","x":200,"y":10,"width":80,"height":60},
        {"type":"line","x1":10,"y1":100,"x2":150,"y2":140,"line_style":"dashed"},
        {"type":"arrow","x1":200,"y1":150,"x2":300,"y2":100,"kind":"tapered"},
        {"type":"text","x":10,"y":160,"text":"hello"},
        {"type":"label","x":10,"y":200,"text":"label"},
        {"type":"callout","x":150,"y":200,"text":"callout","tail_x":140,"tail_y":260},
        {"type":"highlight","x":10,"y":250,"width":120,"height":20},
        {"type":"blur","x":250,"y":200,"width":80,"height":50,"effect":"pixelate"},
        {"type":"spotlight","x":300,"y":10,"width":80,"height":60,"dim":0.3},
        {"type":"counter","x":350,"y":150},
        {"type":"watermark","text":"TEST","style":"tiled","opacity":0.2},
        {"type":"pencil","points":[[300,250],[320,270],[340,250]]}
    ]);
    let out = home.join("Pictures/Screenshots/out.png");
    let (err, text) = s.tool(
        "annotate",
        json!({"path": src, "output": out, "items": items, "return_image": false, "background": "blue purple", "padding": 20}),
    );
    assert!(!err, "{text}");
    let meta: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(meta["items"], 14);
    assert_eq!(meta["width"], 440);
    assert!(out.exists());

    // Existing file is not replaced without overwrite.
    let (err, text) = s.tool("annotate", json!({"path": src, "output": out, "items": [], "return_image": false}));
    assert!(err && text.contains("overwrite"), "{text}");
    // Writes outside the fence are refused.
    let (err, text) = s.tool("annotate", json!({"path": src, "output": home.join("escape.png"), "items": [], "return_image": false}));
    assert!(err && text.contains("refusing"), "{text}");
    // Non-image extensions are refused.
    let (err, _) =
        s.tool("annotate", json!({"path": src, "output": home.join("Pictures/Screenshots/x.txt"), "items": [], "return_image": false}));
    assert!(err);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn compare_reports_changed_regions() {
    let home = scratch();
    let a = home.join("Pictures/Screenshots/a.png");
    let b = home.join("Pictures/Screenshots/b.png");
    blank_png(&a, 200, 200);
    let mut img = image::RgbaImage::from_pixel(200, 200, image::Rgba([250, 250, 250, 255]));
    for y in 50..100 {
        for x in 20..120 {
            img.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
        }
    }
    img.save(&b).unwrap();
    let mut s = Server::start(&[], &home);
    let (err, text) = s.tool("compare", json!({"path_a": a, "path_b": b, "return_image": false}));
    assert!(!err, "{text}");
    let meta: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(meta["identical"], false);
    assert_eq!(meta["changed_regions"], 1);
    let r = &meta["regions"][0];
    assert!(r["x"].as_f64().unwrap() <= 20.0 && r["y"].as_f64().unwrap() <= 50.0);
    assert!(home.join("Pictures/Screenshots/a-diff.png").exists());
    let (_, text) = s.tool("compare", json!({"path_a": a, "path_b": a, "return_image": false, "overwrite": true}));
    let meta: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(meta["identical"], true);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn config_tools_validate_and_protect_the_fence() {
    let home = scratch();
    let mut s = Server::start(&[], &home);
    let (err, _) = s.tool("set_config", json!({"changes": {"general": {"quality": 85}}}));
    assert!(!err);
    let (_, text) = s.tool("get_config", json!({}));
    assert!(text.contains("quality = 85"));
    let (err, _) = s.tool("set_config", json!({"changes": {"general": {"format": "bmp"}}}));
    assert!(err, "unknown format must be rejected");
    let (err, _) = s.tool("set_config", json!({"changes": {"general": {"filename_pattern": "%Q/../x"}}}));
    assert!(err, "bad strftime / path separators must be rejected");
    let (err, text) = s.tool("set_config", json!({"changes": {"general": {"save_folder": "/tmp"}}}));
    assert!(err && text.contains("save_folder"), "fence keys are protected");
    let (err, _) = s.tool("set_config", json!({"changes": {"annotate": {"default_background": "wallpaper"}}}));
    assert!(!err);
    let (err, _) = s.tool("set_config", json!({"changes": {"annotate": {"default_background": "sparkles"}}}));
    assert!(err);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn read_only_mode_hides_writing_tools() {
    let home = scratch();
    let mut s = Server::start(&["--read-only"], &home);
    let tools = s.call("tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    for hidden in ["annotate", "redact", "set_config"] {
        assert!(!names.contains(&hidden), "{hidden} must be hidden in read-only mode");
    }
    let src = home.join("Pictures/Screenshots/src.png");
    blank_png(&src, 50, 50);
    let (err, text) = s.tool("annotate", json!({"path": src, "items": []}));
    assert!(err && text.contains("read-only"));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn describe_annotations_is_a_reference() {
    let home = scratch();
    let mut s = Server::start(&[], &home);
    let (err, text) = s.tool("describe_annotations", json!({}));
    assert!(!err);
    for word in ["rect", "arrow", "callout", "blur", "spotlight", "watermark", "wallpaper"] {
        assert!(text.contains(word), "reference should mention {word}");
    }
    let _ = std::fs::remove_dir_all(&home);
}
