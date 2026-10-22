//! Codex MCP registration in the user config file.

use std::io;

use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, TableLike, Value as TomlValue};

use super::{
    exe_string, outcome, read_if_present, skipped, write_new_file, Existing, RegistrationOptions,
    RegistrationOutcome, SERVER_NAME, MCP_ARG, TOOLS_ENV,
};

/// Merges `[mcp_servers.medulla]` into `<root>/.codex/config.toml`.
///
/// Edited with `toml_edit`, not `toml`: `toml::Table` models values only, so
/// reading a config into one and writing it back deletes every comment and
/// re-sorts every key. `~/.codex/config.toml` is a file the operator wrote by
/// hand, and the very first registration would have eaten it. `toml_edit` keeps
/// the document's trivia, so the write is an *edit* — our table appended or its
/// changed keys reassigned, and nothing else in the file moved.
pub(super) fn register_codex(opts: &RegistrationOptions) -> io::Result<RegistrationOutcome> {
    let path = opts.root.join(".codex").join("config.toml");
    let existing = match read_if_present(&path)? {
        Existing::Absent => None,
        Existing::Unreadable => {
            return Ok(skipped(
                SkillTarget::Codex,
                &path,
                format!(
                    "{} is not valid UTF-8; left untouched — add [mcp_servers.medulla] by hand \
                     or move the file aside",
                    path.display()
                ),
            ));
        }
        Existing::Text(text) => Some(text),
    };

    let mut document = match existing.as_deref() {
        None => DocumentMut::new(),
        Some(text) => match text.parse::<DocumentMut>() {
            Ok(document) => document,
            Err(error) => {
                return Ok(skipped(
                    SkillTarget::Codex,
                    &path,
                    format!("could not parse existing config ({error}); left untouched"),
                ));
            }
        },
    };

    // A fresh `mcp_servers` is implicit so a first registration renders one
    // `[mcp_servers.medulla]` header rather than a bare `[mcp_servers]` above it.
    if document.get("mcp_servers").is_none() {
        let mut table = Table::new();
        table.set_implicit(true);
        document.insert("mcp_servers", Item::Table(table));
    }
    let Some(servers) = document["mcp_servers"].as_table_like_mut() else {
        return Ok(skipped(
            SkillTarget::Codex,
            &path,
            "`mcp_servers` exists but is not a table; fix it and re-run".to_string(),
        ));
    };

    if codex_entry_matches(servers.get(SERVER_NAME), opts) {
        return Ok(outcome(SkillTarget::Codex, path, "unchanged"));
    }
    let action = if existing.is_some() {
        "updated"
    } else {
        "created"
    };
    write_codex_entry(servers, opts);

    if !opts.dry_run {
        write_new_file(&path, &document.to_string())?;
    }
    Ok(outcome(SkillTarget::Codex, path, action))
}

/// Whether `item` already says everything [`write_codex_entry`] would write.
///
/// Compared by meaning, not by rendering: an operator who spelled the entry as
/// an inline table, or `env` as a sub-table, is saying the same thing and must
/// not be reformatted into our spelling. Keys we do not write are ignored rather
/// than counted as a difference — see [`write_codex_entry`] for what is ours.
fn codex_entry_matches(item: Option<&Item>, opts: &RegistrationOptions) -> bool {
    let Some(entry) = item.and_then(Item::as_table_like) else {
        return false;
    };
    command_matches(entry, opts) && args_matches(entry) && env_matches(entry, opts)
}

/// Whether the entry already points at this binary.
fn command_matches(entry: &dyn TableLike, opts: &RegistrationOptions) -> bool {
    entry.get("command").and_then(Item::as_str) == Some(&exe_string(opts))
}

/// Whether the entry's argv is exactly `["mcp"]`.
fn args_matches(entry: &dyn TableLike) -> bool {
    entry
        .get("args")
        .and_then(Item::as_array)
        .is_some_and(|args| {
            args.len() == 1 && args.get(0).and_then(TomlValue::as_str) == Some(MCP_ARG)
        })
}

/// Whether the entry's env already selects this tool mode.
///
/// Other variables in the same table are not our business, so they are not read
/// here and not written below.
fn env_matches(entry: &dyn TableLike, opts: &RegistrationOptions) -> bool {
    entry
        .get("env")
        .and_then(Item::as_table_like)
        .is_some_and(|env| env.get(TOOLS_ENV).and_then(Item::as_str) == Some(&opts.tools_mode))
}

/// Writes `[mcp_servers.medulla]` into `servers`, touching as little as possible.
///
/// Three keys are ours and are corrected: `command` and `args`, which *are* the
/// process this registration promises the harness, and the [`TOOLS_ENV`]
/// variable, which is the tool surface the operator chose. Everything else the
/// operator put under our table — a `startup_timeout_sec`, another environment
/// variable — is theirs and is left alone.
///
/// Even those three are only assigned when they do not already mean the right
/// thing, because assigning replaces the key's value and any trailing comment on
/// its line. A registration that only moved the binary therefore leaves the rest
/// of the table, and the table's position in the operator's file, as it was.
fn write_codex_entry(servers: &mut dyn TableLike, opts: &RegistrationOptions) {
    if !servers
        .get(SERVER_NAME)
        .is_some_and(|item| item.is_table_like())
    {
        servers.insert(SERVER_NAME, Item::Table(Table::new()));
    }
    let Some(entry) = servers
        .get_mut(SERVER_NAME)
        .and_then(Item::as_table_like_mut)
    else {
        // Unreachable: the branch above just made it a table.
        return;
    };

    if !command_matches(entry, opts) {
        entry.insert("command", toml_edit::value(exe_string(opts)));
    }
    if !args_matches(entry) {
        let mut args = Array::new();
        args.push(MCP_ARG);
        entry.insert("args", toml_edit::value(args));
    }
    if !env_matches(entry, opts) {
        write_tools_env(entry, opts);
    }
}

/// Sets [`TOOLS_ENV`] inside the entry's `env`, creating the table if needed.
///
/// Written into the operator's existing table rather than over it, so a session
/// they gave another variable keeps it when the tool mode changes.
fn write_tools_env(entry: &mut dyn TableLike, opts: &RegistrationOptions) {
    if !entry.get("env").is_some_and(|item| item.is_table_like()) {
        entry.insert("env", toml_edit::value(InlineTable::new()));
    }
    if let Some(env) = entry.get_mut("env").and_then(Item::as_table_like_mut) {
        env.insert(TOOLS_ENV, toml_edit::value(opts.tools_mode.clone()));
    }
}

