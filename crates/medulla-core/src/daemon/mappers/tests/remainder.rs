#[test]
fn codex_new_reasoning_item_uses_text_then_summary_fallback() {
    // A `reasoning` item surfaces as agent_thinking. It prefers its own `text`,
    // and falls back to the `summary` content when `text` is absent — codex emits
    // one shape or the other depending on the model.
    let direct = map_all(
        "codex",
        &[
            r#"{"type":"item.completed","item":{"id":"r1","type":"reasoning","text":"thinking hard"}}"#,
        ],
    );
    assert_eq!(kind_of(&direct[0]), "agent_thinking");
    assert_eq!(direct[0].event.payload["text"], "thinking hard");

    let via_summary = map_all(
        "codex",
        &[
            r#"{"type":"item.completed","item":{"id":"r2","type":"reasoning","summary":[{"type":"summary_text","text":"a plan"}]}}"#,
        ],
    );
    assert_eq!(kind_of(&via_summary[0]), "agent_thinking");
    assert_eq!(via_summary[0].event.payload["text"], "a plan");
}

#[test]
fn codex_new_stream_ignores_empty_and_unactionable_items() {
    // Items that carry no text we would surface produce nothing rather than an
    // empty event: a reasoning/message item with no text, an item.started message
    // (the reply is emitted once, on completion), an item type we do not map, and
    // a record whose `item` object is missing entirely.
    let events = map_all(
        "codex",
        &[
            r#"{"type":"item.completed","item":{"id":"r3","type":"reasoning"}}"#,
            r#"{"type":"item.completed","item":{"id":"a1","type":"agent_message","text":""}}"#,
            r#"{"type":"item.started","item":{"id":"a2","type":"agent_message","text":"partial"}}"#,
            r#"{"type":"item.completed","item":{"id":"x1","type":"file_change"}}"#,
            r#"{"type":"item.completed"}"#,
        ],
    );
    assert!(
        events.is_empty(),
        "none of these items should map: {events:?}"
    );
}

#[test]
fn codex_legacy_response_item_reasoning_falls_back_to_content() {
    // The older `response_item` reasoning path: when `summary` is empty, the text
    // comes from `content` (reasoning_text), still surfaced as agent_thinking.
    let events = map_all(
        "codex",
        &[
            r#"{"type":"response_item","payload":{"type":"reasoning","summary":[],"content":[{"type":"reasoning_text","text":"old reasoning"}]}}"#,
        ],
    );
    assert_eq!(kind_of(&events[0]), "agent_thinking");
    assert_eq!(events[0].event.payload["text"], "old reasoning");
}

#[test]
fn opencode_flat_text_tool_and_error() {
    let text = r#"{"type":"text","part":{"type":"text","text":"working on it"}}"#;
    let tool_call = r#"{"type":"tool","part":{"type":"tool","tool":"read","callID":"r1","state":{"status":"running","input":{"file_path":"/a/b.rs"}}}}"#;
    let tool_result = r#"{"type":"tool","part":{"type":"tool","tool":"read","callID":"r1","state":{"status":"completed","output":"contents"}}}"#;
    let error =
        r#"{"type":"error","error":{"name":"ProviderError","data":{"message":"no creds"}}}"#;
    let events = map_all("opencode", &[text, tool_call, tool_result, error]);
    assert_eq!(kind_of(&events[0]), "agent_message");
    assert_eq!(events[0].event.payload["text"], "working on it");
    assert_eq!(kind_of(&events[1]), "tool_call");
    assert_eq!(events[1].event.payload["tool_kind"], "file_read");
    assert_eq!(events[1].event.payload["display"], "/a/b.rs");
    assert_eq!(kind_of(&events[2]), "tool_result");
    assert_eq!(events[2].event.payload["output"], "contents");
    assert_eq!(kind_of(&events[3]), "error");
    assert_eq!(
        events[3].event.payload["message"],
        "ProviderError: no creds"
    );
}

#[test]
fn normalize_tool_kind_ladder() {
    assert_eq!(normalize_tool_kind("mcp__github__list"), "mcp");
    assert_eq!(normalize_tool_kind("Bash"), "shell");
    assert_eq!(normalize_tool_kind("MultiEdit"), "edit");
    assert_eq!(normalize_tool_kind("Write"), "file_write");
    assert_eq!(normalize_tool_kind("Read"), "file_read");
    assert_eq!(normalize_tool_kind("Grep"), "search");
    assert_eq!(normalize_tool_kind("WebFetch"), "web");
    assert_eq!(normalize_tool_kind("Task"), "task");
    assert_eq!(normalize_tool_kind("Xyzzy"), "other");
}

#[test]
fn truncate_caps_on_byte_boundary() {
    let big = "x".repeat(OUTPUT_CAP + 100);
    let out = truncate(&big);
    assert!(out.ends_with(ELISION));
    assert!(out.len() <= OUTPUT_CAP + ELISION.len());
}

#[test]
fn malformed_and_empty_lines_yield_nothing() {
    assert!(map_all("claude", &["not json"]).is_empty());
    assert!(map_all("claude", &["[1,2,3]"]).is_empty()); // not an object
    assert!(map_all("codex", &[r#"{"type":"event_msg"}"#]).is_empty()); // no payload
    assert!(map_all("opencode", &[r#"{"type":"text"}"#]).is_empty()); // no part
                                                                      // An unknown provider maps to opencode semantics but unknown records drop.
    assert!(map_all("mystery", &[r#"{"type":"other","part":{}}"#]).is_empty());
}

#[test]
fn claude_empty_text_and_thinking_fallback() {
    // Empty assistant text produces no event.
    let empty = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":""}]}}"#;
    assert!(map_all("claude", &[empty]).is_empty());

    // A thinking block falling back to the `text` field.
    let thinking = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","text":"pondering"}]}}"#;
    let events = map_all("claude", &[thinking]);
    assert_eq!(kind_of(&events[0]), "agent_thinking");
    assert_eq!(events[0].event.payload["text"], "pondering");

    // A user string prompt (non-array content) and the empty-string case.
    let empty_prompt = r#"{"type":"user","message":{"role":"user","content":""}}"#;
    assert!(map_all("claude", &[empty_prompt]).is_empty());

    // A tool_result with structured array content is flattened + joined.
    let result = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t9","is_error":true,"content":[{"type":"text","text":"line1"},{"type":"text","text":"line2"}]}]}}"#;
    let events = map_all("claude", &[result]);
    assert_eq!(kind_of(&events[0]), "tool_result");
    assert_eq!(events[0].event.payload["is_error"], true);
    assert_eq!(events[0].event.payload["output"], "line1\nline2");
}

#[test]
fn codex_reasoning_mcp_and_search_variants() {
    // Reasoning falling back from summary to content.
    let reasoning = r#"{"type":"response_item","payload":{"type":"reasoning","content":[{"type":"reasoning_text","text":"deep thought"}]}}"#;
    let events = map_all("codex", &[reasoning]);
    assert_eq!(kind_of(&events[0]), "agent_thinking");
    assert_eq!(events[0].event.payload["text"], "deep thought");

    // MCP tool begin overrides tool_kind to "mcp".
    let mcp_begin = r#"{"type":"response_item","payload":{"type":"mcp_tool_call_begin","tool":"lookup","call_id":"m1","arguments":"{\"q\":\"x\"}"}}"#;
    let events = map_all("codex", &[mcp_begin]);
    assert_eq!(kind_of(&events[0]), "tool_call");
    assert_eq!(events[0].event.payload["tool_kind"], "mcp");

    // MCP tool end with a nested error output.
    let mcp_end = r#"{"type":"response_item","payload":{"type":"mcp_tool_call_end","call_id":"m1","output":{"is_error":true,"content":"boom"}}}"#;
    let events = map_all("codex", &[mcp_end]);
    assert_eq!(kind_of(&events[0]), "tool_result");
    assert_eq!(events[0].event.payload["is_error"], true);
    assert_eq!(events[0].event.payload["output"], "boom");

    // A tool_search_call driven off the `query` field.
    let search =
        r#"{"type":"response_item","payload":{"type":"tool_search_call","query":"ripgrep foo"}}"#;
    let events = map_all("codex", &[search]);
    assert_eq!(kind_of(&events[0]), "tool_call");
    assert_eq!(events[0].event.payload["tool_name"], "ripgrep foo");

    // task_complete → idle status.
    let complete = r#"{"type":"event_msg","payload":{"type":"task_complete"}}"#;
    let events = map_all("codex", &[complete]);
    assert_eq!(kind_of(&events[0]), "status");
    assert_eq!(events[0].event.payload["state"], "idle");
}

#[test]
fn opencode_reasoning_error_ref_and_running_tool() {
    let reasoning = r#"{"type":"reasoning","part":{"type":"reasoning","text":"  thinking  "}}"#;
    let events = map_all("opencode", &[reasoning]);
    assert_eq!(kind_of(&events[0]), "agent_thinking");
    assert_eq!(events[0].event.payload["text"], "thinking");

    // An error with a name + ref suffix.
    let error =
        r#"{"type":"error","error":{"name":"AuthError","data":{"message":"denied","ref":"E42"}}}"#;
    let events = map_all("opencode", &[error]);
    assert_eq!(
        events[0].event.payload["message"],
        "AuthError: denied (E42)"
    );

    // A running (non-terminal) tool with no output → a tool_call, not result.
    let running = r#"{"type":"tool","part":{"type":"tool","tool":"bash","callID":"b1","state":{"status":"running","input":{"command":"ls"}}}}"#;
    let events = map_all("opencode", &[running]);
    assert_eq!(kind_of(&events[0]), "tool_call");
    assert_eq!(events[0].event.payload["tool_kind"], "shell");
}

#[test]
fn tool_display_and_bound_input_helpers() {
    // Falls back to the tool name when no known key is present.
    assert_eq!(tool_display("Weird", &serde_json::json!({})), "Weird");
    // A bare string input is used directly.
    assert_eq!(
        tool_display("X", &serde_json::json!("just a string")),
        "just a string"
    );
    // Oversized structured input collapses to a byte-capped string.
    let big = "y".repeat(INPUT_CAP + 50);
    let bounded = bound_tool_input(&serde_json::json!({ "blob": big }));
    assert!(bounded.is_string());
    assert!(bounded.as_str().unwrap().ends_with(ELISION));
}

#[test]
fn parse_iso_rejects_garbage() {
    assert!(parse_iso_to_ms("nope").is_none());
    assert!(parse_iso_to_ms("2026/07/05").is_none());
    // A receive-time fallback is used when the field is absent (non-panicking).
    let ts = parse_timestamp_ms(None);
    assert!(ts > 0);
}
#[test]
fn parses_iso_timestamp() {
    // 2026-07-05T00:00:00Z is 1_783_209_600 s since the Unix epoch.
    let ms = parse_iso_to_ms("2026-07-05T00:00:00Z").unwrap();
    assert_eq!(ms, 1_783_209_600_000);
    let with_ms = parse_iso_to_ms("2026-07-05T00:00:00.500Z").unwrap();
    assert_eq!(with_ms, 1_783_209_600_500);
    // Offset handling: 01:00+01:00 is the same instant as 00:00Z.
    let offset = parse_iso_to_ms("2026-07-05T01:00:00+01:00").unwrap();
    assert_eq!(offset, 1_783_209_600_000);
}

#[test]
fn failed_pr_commands_do_not_publish_a_url() {
    for (provider, lines) in [
        (
            "claude",
            vec![
                json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"p","name":"Bash","input":{"command":"gh pr create --fill"}}]}}),
                json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"p","is_error":true,"content":"https://github.com/acme/repo/pull/42"}]}}),
            ],
        ),
        (
            "codex",
            vec![
                json!({"type":"item.started","item":{"id":"p","type":"command_execution","command":"gh pr create --fill"}}),
                json!({"type":"item.completed","item":{"id":"p","type":"command_execution","command":"gh pr create --fill","aggregated_output":"https://github.com/acme/repo/pull/42","exit_code":1}}),
            ],
        ),
    ] {
        let mut mapper = HarnessLineMapper::new_with_gh_repo_override(provider, false);
        let events = lines
            .iter()
            .enumerate()
            .flat_map(|(line, value)| mapper.map_line(&value.to_string(), line as i64));
        assert!(
            !events
                .into_iter()
                .any(|event| { event.event.kind == crate::harness_work::kinds::SESSION_INFO }),
            "{provider}"
        );
    }
}

#[test]
fn codex_turn_failed_carries_the_reason_beside_the_idle_status() {
    // Captured verbatim from codex-cli 0.150.1 against an endpoint answering
    // 402: the reason appears only here, on stdout — stderr holds nothing but
    // codex's startup chatter, so a caller keying off the stderr tail reports
    // the wrong cause.
    let mut mapper = HarnessLineMapper::new_with_gh_repo_override("codex", false);
    let detail = "unexpected status 402 Payment Required: Insufficient credits.";
    let events = mapper.map_line(
        &format!(r#"{{"type":"turn.failed","error":{{"message":"{detail}"}}}}"#),
        0,
    );

    let kinds: Vec<&str> = events.iter().map(kind_of).collect();
    assert_eq!(kinds, vec!["status", "error"]);
    assert_eq!(events[0].event.payload["state"], "idle");
    assert_eq!(events[1].event.payload["message"], detail);
    assert_eq!(events[1].event.payload["fatal"], true);
}

#[test]
fn codex_turn_failed_without_a_message_still_reports_the_failure() {
    let mut mapper = HarnessLineMapper::new_with_gh_repo_override("codex", false);
    let events = mapper.map_line(r#"{"type":"turn.failed"}"#, 0);

    assert_eq!(
        events.iter().map(kind_of).collect::<Vec<_>>(),
        vec!["status", "error"]
    );
    assert_eq!(events[1].event.payload["fatal"], true);
}

#[test]
fn codex_usage_follows_the_cumulative_total_not_the_last_call() {
    // A real `token_count` record carries both the latest call's usage and the
    // running total; `last_token_usage` comes first, and it is the total that
    // the cumulative fold must track.
    let record = |last_in: i64, total_in: i64, total_out: i64| {
        json!({
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": {
                    "last_token_usage": { "input_tokens": last_in, "output_tokens": 1 },
                    "total_token_usage": { "input_tokens": total_in, "output_tokens": total_out },
                },
            },
        })
        .to_string()
    };
    let mut mapper = HarnessLineMapper::new("codex");
    mapper.map_line(&record(7_713, 7_713, 10), 1);
    mapper.map_line(&record(8_176, 15_889, 20), 2);
    assert_eq!(
        mapper.usage(),
        Some(TokenUsage {
            input_tokens: 15_889,
            output_tokens: 20,
            ..Default::default()
        })
    );
}

#[test]
fn an_invalid_codex_total_is_skipped_not_replaced_by_the_last_call() {
    let mut mapper = HarnessLineMapper::new("codex");
    mapper.map_line(
        &json!({
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": {
                    "last_token_usage": { "input_tokens": 5, "output_tokens": 1 },
                    "total_token_usage": { "input_tokens": -1, "output_tokens": 2 },
                },
            },
        })
        .to_string(),
        1,
    );
    assert_eq!(mapper.usage(), None);
}
