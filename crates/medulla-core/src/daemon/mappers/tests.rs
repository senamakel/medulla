//! Unit tests for the JSONL line mappers: the token-usage scan, the per-provider
//! folds (claude/codex/opencode), the codex dedupe, the shared tool helpers, and
//! the RFC3339 timestamp parser.

use serde_json::json;

use crate::protocol::TokenUsage;

use super::shared::{
    bound_tool_input, normalize_tool_kind, tool_display, truncate, ELISION, INPUT_CAP, OUTPUT_CAP,
};
use super::timestamp::{parse_iso_to_ms, parse_timestamp_ms};
use super::types::{HarnessLineMapper, HarnessSemanticEvent};

#[test]
fn mapper_accumulates_latest_usage() {
    let mut mapper = HarnessLineMapper::new_with_gh_repo_override("codex", false);
    assert_eq!(mapper.usage(), None);
    let _ = mapper.map_line(
        r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":5,"output_tokens":1}}}}"#,
        0,
    );
    let _ = mapper.map_line(
        r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":50,"output_tokens":11}}}}"#,
        1,
    );
    assert_eq!(
        mapper.usage(),
        Some(TokenUsage {
            input_tokens: 50,
            output_tokens: 11,
            ..Default::default()
        })
    );
}

#[test]
fn opencode_step_usage_is_summed_instead_of_delta_folded() {
    let mut mapper = HarnessLineMapper::new_with_gh_repo_override("opencode", false);
    for (input, output) in [(100, 10), (120, 12)] {
        let line =
            format!(r#"{{"type":"step-finish","tokens":{{"input":{input},"output":{output}}}}}"#);
        let _ = mapper.map_line(&line, 0);
    }
    assert_eq!(
        mapper.usage(),
        Some(TokenUsage {
            input_tokens: 220,
            output_tokens: 22,

            cache_read: None,
            cache_write: None,
            reasoning: None,
        })
    );
}

#[test]
fn harness_usage_categories_survive_stream_folding() {
    let mut claude = HarnessLineMapper::new_with_gh_repo_override("claude", false);
    let _ = claude.map_line(
        r#"{"type":"assistant","message":{"usage":{"input_tokens":100,"output_tokens":20,"cache_read_input_tokens":30,"cache_creation_input_tokens":5},"content":[]}}"#,
        0,
    );
    assert_eq!(claude.usage().unwrap().cache_read, Some(30));
    assert_eq!(claude.usage().unwrap().cache_write, Some(5));

    let mut codex = HarnessLineMapper::new_with_gh_repo_override("codex", false);
    let _ = codex.map_line(
        r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":30,"cache_write_input_tokens":5,"output_tokens":20,"reasoning_output_tokens":7}}}}"#,
        0,
    );
    let codex_usage = codex.usage().unwrap();
    assert_eq!(codex_usage.cache_read, Some(30));
    assert_eq!(codex_usage.cache_write, Some(5));
    assert_eq!(codex_usage.reasoning, Some(7));

    let mut opencode = HarnessLineMapper::new_with_gh_repo_override("opencode", false);
    let _ = opencode.map_line(
        r#"{"type":"part","part":{"tokens":{"input":100,"output":20,"reasoning":7,"cache":{"read":30,"write":5}}}}"#,
        0,
    );
    let opencode_usage = opencode.usage().unwrap();
    assert_eq!(opencode_usage.cache_read, Some(30));
    assert_eq!(opencode_usage.cache_write, Some(5));
    assert_eq!(opencode_usage.reasoning, Some(7));
}

#[test]
fn harness_usage_categories_follow_provider_folding_rules() {
    let mut claude = HarnessLineMapper::new_with_gh_repo_override("claude", false);
    for line in [
        r#"{"type":"assistant","message":{"usage":{"input_tokens":10,"output_tokens":2,"cache_read_input_tokens":3,"cache_creation_input_tokens":1},"content":[]}}"#,
        r#"{"type":"assistant","message":{"usage":{"input_tokens":20,"output_tokens":4,"cache_read_input_tokens":5,"cache_creation_input_tokens":2},"content":[]}}"#,
    ] {
        let _ = claude.map_line(line, 0);
    }
    assert_eq!(
        claude.usage(),
        Some(TokenUsage {
            input_tokens: 30,
            output_tokens: 6,
            cache_read: Some(8),
            cache_write: Some(3),
            reasoning: None,
        })
    );

    let mut codex = HarnessLineMapper::new_with_gh_repo_override("codex", false);
    for line in [
        r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":3,"cache_write_input_tokens":1,"output_tokens":2,"reasoning_output_tokens":1}}}}"#,
        r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":20,"cached_input_tokens":5,"cache_write_input_tokens":2,"output_tokens":4,"reasoning_output_tokens":2}}}}"#,
    ] {
        let _ = codex.map_line(line, 0);
    }
    assert_eq!(
        codex.usage(),
        Some(TokenUsage {
            input_tokens: 20,
            output_tokens: 4,
            cache_read: Some(5),
            cache_write: Some(2),
            reasoning: Some(2),
        })
    );

    let mut opencode = HarnessLineMapper::new_with_gh_repo_override("opencode", false);
    for line in [
        r#"{"type":"step-finish","tokens":{"input":10,"output":2,"reasoning":1,"cache":{"read":3,"write":1}}}"#,
        r#"{"type":"step-finish","tokens":{"input":20,"output":4,"reasoning":2,"cache":{"read":5,"write":2}}}"#,
    ] {
        let _ = opencode.map_line(line, 0);
    }
    assert_eq!(
        opencode.usage(),
        Some(TokenUsage {
            input_tokens: 30,
            output_tokens: 6,
            cache_read: Some(8),
            cache_write: Some(3),
            reasoning: Some(3),
        })
    );
}

#[test]
fn claude_assistant_call_usage_is_summed_and_result_total_is_not_counted_twice() {
    let mut mapper = HarnessLineMapper::new_with_gh_repo_override("claude", false);
    for (input, output) in [(8, 900), (12, 1500), (6, 700)] {
        let line = format!(
            r#"{{"type":"assistant","message":{{"usage":{{"input_tokens":{input},"output_tokens":{output}}},"content":[]}}}}"#
        );
        let _ = mapper.map_line(&line, 0);
    }
    let _ = mapper.map_line(
        r#"{"type":"result","usage":{"input_tokens":26,"output_tokens":3100}}"#,
        3,
    );
    assert_eq!(
        mapper.usage(),
        Some(TokenUsage {
            input_tokens: 26,
            output_tokens: 3100,

            cache_read: None,
            cache_write: None,
            reasoning: None,
        })
    );
}

fn map_all(provider: &str, lines: &[&str]) -> Vec<HarnessSemanticEvent> {
    let mut mapper = HarnessLineMapper::new_with_gh_repo_override(provider, false);
    let mut out = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        out.extend(mapper.map_line(line, index as i64));
    }
    out
}

fn kind_of(event: &HarnessSemanticEvent) -> &str {
    &event.event.kind
}

#[test]
fn claude_user_prompt_and_tool_use_and_result() {
    let user = r#"{"type":"user","timestamp":"2026-07-05T00:00:00Z","message":{"role":"user","content":"do the thing"}}"#;
    let assistant = r#"{"type":"assistant","timestamp":"2026-07-05T00:00:01Z","message":{"role":"assistant","content":[{"type":"text","text":"on it"},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls -la"}}]}}"#;
    let result = r#"{"type":"user","timestamp":"2026-07-05T00:00:02Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","is_error":false,"content":"file1\nfile2"}]}}"#;

    let events = map_all("claude", &[user, assistant, result]);
    assert_eq!(kind_of(&events[0]), "user_prompt");
    assert_eq!(events[0].event.payload["text"], "do the thing");
    assert_eq!(events[0].event.role, "owner");
    assert_eq!(kind_of(&events[1]), "agent_message");
    assert_eq!(kind_of(&events[2]), "tool_call");
    assert_eq!(events[2].event.payload["tool_kind"], "shell");
    assert_eq!(events[2].event.payload["display"], "ls -la");
    assert_eq!(events[2].event.payload["call_id"], "t1");
    assert_eq!(kind_of(&events[3]), "tool_result");
    assert_eq!(events[3].event.payload["ok"], true);
    assert_eq!(events[3].event.payload["output"], "file1\nfile2");
    assert_eq!(events[3].event.payload["call_id"], "t1");
}

#[test]
fn claude_only_marks_the_final_assistant_round_as_turn_end() {
    let tool_use = r#"{"type":"assistant","timestamp":"2026-08-16T08:37:15.404Z","message":{"role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"true"}}]}}"#;
    let end_turn = r#"{"type":"assistant","timestamp":"2026-08-16T08:37:24.506Z","message":{"role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"done"}]}}"#;
    let stop_sequence = r#"{"type":"assistant","timestamp":"2026-08-16T08:37:24.506Z","message":{"role":"assistant","stop_reason":"stop_sequence","content":[{"type":"text","text":"halted"}]}}"#;
    let incomplete = r#"{"type":"assistant","timestamp":"2026-08-16T08:37:24.506Z","message":{"role":"assistant","content":[{"type":"text","text":"still streaming"}]}}"#;

    let tool_events = map_all("claude", &[tool_use]);
    assert_eq!(tool_events.len(), 1);
    assert_eq!(kind_of(&tool_events[0]), "tool_call");

    let final_events = map_all("claude", &[end_turn]);
    assert_eq!(final_events.len(), 2);
    assert_eq!(kind_of(&final_events[0]), "agent_message");
    assert_eq!(kind_of(&final_events[1]), "lifecycle");
    assert_eq!(final_events[1].event.payload["phase"], "turn_end");
    assert_eq!(final_events[1].record_type, "assistant:end_turn");

    let stopped_events = map_all("claude", &[stop_sequence]);
    assert_eq!(stopped_events.len(), 2);
    assert_eq!(kind_of(&stopped_events[1]), "lifecycle");
    assert_eq!(stopped_events[1].event.payload["phase"], "turn_end");

    let incomplete_events = map_all("claude", &[incomplete]);
    assert_eq!(incomplete_events.len(), 1);
    assert_eq!(kind_of(&incomplete_events[0]), "agent_message");
}

#[test]
fn codex_dedupes_double_recorded_agent_message() {
    let event_msg = r#"{"type":"event_msg","timestamp":"2026-07-05T00:00:00.000Z","payload":{"type":"agent_message","message":"final answer"}}"#;
    let response_item = r#"{"type":"response_item","timestamp":"2026-07-05T00:00:00.500Z","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"final answer"}]}}"#;
    let events = map_all("codex", &[event_msg, response_item]);
    let messages: Vec<_> = events
        .iter()
        .filter(|e| kind_of(e) == "agent_message")
        .collect();
    assert_eq!(
        messages.len(),
        1,
        "duplicate agent_message should be dropped"
    );
    assert_eq!(messages[0].event.payload["text"], "final answer");
}

#[test]
fn codex_function_call_and_output_and_status() {
    let call = r#"{"type":"response_item","timestamp":"2026-07-05T00:00:00Z","payload":{"type":"function_call","name":"shell","call_id":"c1","arguments":"{\"command\":\"npm test\"}"}}"#;
    let output = r#"{"type":"response_item","timestamp":"2026-07-05T00:00:01Z","payload":{"type":"function_call_output","call_id":"c1","output":"ok","success":true}}"#;
    let started = r#"{"type":"event_msg","timestamp":"2026-07-05T00:00:02Z","payload":{"type":"task_started"}}"#;
    let events = map_all("codex", &[call, output, started]);
    assert_eq!(kind_of(&events[0]), "tool_call");
    assert_eq!(events[0].event.payload["display"], "npm test");
    assert_eq!(events[0].event.payload["tool_kind"], "shell");
    assert_eq!(kind_of(&events[1]), "tool_result");
    assert_eq!(events[1].event.payload["ok"], true);
    assert_eq!(kind_of(&events[2]), "status");
    assert_eq!(events[2].event.payload["state"], "running");
    assert_eq!(events[2].event.payload["detail"], "working");
}

#[test]
fn codex_marks_error_result() {
    let output = r#"{"type":"response_item","timestamp":"2026-07-05T00:00:01Z","payload":{"type":"function_call_output","call_id":"c1","output":"boom","success":false}}"#;
    let events = map_all("codex", &[output]);
    assert_eq!(kind_of(&events[0]), "tool_result");
    assert_eq!(events[0].event.payload["is_error"], true);
    assert_eq!(events[0].event.payload["ok"], false);
}

#[test]
fn codex_new_exec_json_stream_maps_message_command_and_usage() {
    // Real codex >= 0.140 `exec --json` shapes: a thread/turn/item event stream
    // with the detail on a nested `item` (not the older `event_msg`/`payload`),
    // captured verbatim from codex-cli 0.142.
    let mut mapper = HarnessLineMapper::new_with_gh_repo_override("codex", false);
    let lines = [
        r#"{"type":"thread.started","thread_id":"019f-abc"}"#,
        r#"{"type":"turn.started"}"#,
        r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"BANANA"}}"#,
        r#"{"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"/bin/zsh -lc 'cat note.txt'","aggregated_output":"","exit_code":null,"status":"in_progress"}}"#,
        r#"{"type":"item.completed","item":{"id":"item_1","type":"command_execution","command":"/bin/zsh -lc 'cat note.txt'","aggregated_output":"hello\n","exit_code":0,"status":"completed"}}"#,
        r#"{"type":"turn.completed","usage":{"input_tokens":24659,"cached_input_tokens":19200,"output_tokens":76}}"#,
    ];
    let mut events = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        events.extend(mapper.map_line(line, index as i64));
    }

    // thread.started yields no event; the rest map in order.
    let kinds: Vec<&str> = events.iter().map(kind_of).collect();
    assert_eq!(
        kinds,
        vec![
            "status",
            "agent_message",
            "tool_call",
            "tool_result",
            "status"
        ]
    );

    // The agent message is the reply source.
    assert_eq!(events[1].event.payload["text"], "BANANA");
    // The shell command becomes a tool_call (display = the command) then a
    // tool_result carrying its exit status and output.
    assert_eq!(events[2].event.payload["tool_kind"], "shell");
    assert_eq!(
        events[2].event.payload["display"],
        "/bin/zsh -lc 'cat note.txt'"
    );
    assert_eq!(events[2].event.payload["call_id"], "item_1");
    assert_eq!(events[3].event.payload["ok"], true);
    assert_eq!(events[3].event.payload["output"], "hello\n");
    // turn.started/completed bracket the turn as running/idle status.
    assert_eq!(events[0].event.payload["state"], "running");
    assert_eq!(events[4].event.payload["state"], "idle");

    // Usage is scanned off the `turn.completed` record.
    assert_eq!(
        mapper.usage(),
        Some(TokenUsage {
            input_tokens: 24659,
            output_tokens: 76,
            cache_read: Some(19200),
            ..Default::default()
        })
    );
}

#[test]
fn codex_new_command_nonzero_exit_is_an_error() {
    let events = map_all(
        "codex",
        &[
            r#"{"type":"item.completed","item":{"id":"c9","type":"command_execution","command":"false","aggregated_output":"nope","exit_code":2,"status":"completed"}}"#,
        ],
    );
    assert_eq!(kind_of(&events[0]), "tool_result");
    assert_eq!(events[0].event.payload["is_error"], true);
    assert_eq!(events[0].event.payload["ok"], false);
}

include!("tests/remainder.rs");
