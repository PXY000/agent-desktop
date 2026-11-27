use super::*;
use crate::mcp::catalog;

#[test]
fn compact_surface_exposes_four_tools_with_valid_schemas() {
    let leaf = catalog::tools_with_options(true);
    let defs = definitions(&leaf);
    let names = defs
        .iter()
        .filter_map(|d| d["name"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "desktop_observe",
            "desktop_act",
            "desktop_window",
            "desktop_batch"
        ]
    );
    for def in &defs {
        assert_eq!(def["inputSchema"]["type"], "object");
        assert_eq!(def["inputSchema"]["additionalProperties"], false);
    }
}

#[test]
fn compact_resolve_accepts_both_flat_and_nested_arguments() {
    let leaf = catalog::tools_with_options(true);
    let flat = json!({
        "command": "snapshot",
        "app": "Finder",
        "interactive_only": true,
        "wait_for": "button:OK"
    });
    let (tool, args) = resolve_call("desktop_observe", flat.as_object().unwrap(), &leaf)
        .expect("valid")
        .expect("matched");
    assert_eq!(tool.name, "desktop_snapshot");
    let argv = tool.argv(&args).expect("argv builds");
    assert_eq!(
        argv,
        [
            "--wait-for=button:OK",
            "snapshot",
            "--app=Finder",
            "--interactive-only"
        ]
    );

    let nested = json!({
        "command": "double_click",
        "args": {
            "ref_id": "@s8f3k2p9:e1",
            "headed": true
        }
    });
    let (tool2, args2) = resolve_call("desktop_act", nested.as_object().unwrap(), &leaf)
        .expect("valid")
        .expect("matched");
    assert_eq!(tool2.name, "desktop_double_click");
    let argv2 = tool2.argv(&args2).expect("argv2 builds");
    assert_eq!(argv2, ["--headed", "double-click", "--", "@s8f3k2p9:e1"]);

    let flat_launch = json!({
        "command": "launch",
        "app": "Calculator",
        "args": ["--flag", "val"]
    });
    let (tool3, args3) = resolve_call("desktop_window", flat_launch.as_object().unwrap(), &leaf)
        .expect("valid")
        .expect("matched");
    assert_eq!(tool3.name, "desktop_launch");
    let argv3 = tool3.argv(&args3).expect("argv3 builds");
    assert_eq!(
        argv3,
        ["launch", "--arg=--flag", "--arg=val", "--", "Calculator"]
    );
}

#[test]
fn compact_resolve_rejects_command_in_wrong_group() {
    let leaf = catalog::tools_with_options(false);
    let bad = json!({ "command": "click", "ref_id": "@s8f3k2p9:e1" });
    let err = resolve_call("desktop_observe", bad.as_object().unwrap(), &leaf).unwrap_err();
    assert!(err.contains("not valid for desktop_observe"), "{err}");
}
