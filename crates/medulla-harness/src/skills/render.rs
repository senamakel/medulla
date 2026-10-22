//! Turning a [`SkillWorkflow`] into harness-readable skill text.
//!
//! Rendering is deliberately target-independent: one body serves every harness,
//! and [`super::targets`] decides only where it lands. The generated text has
//! one job — make a model that has never seen this workflow call
//! `mcp__medulla__workflow_run` with the right argument shape, and behave
//! sensibly when the tool is not attached.
//!
//! # Why the text is terse
//!
//! Every installed skill is a standing tax on the operator's context, paid twice
//! over: a harness loads each skill's frontmatter `description` into *every*
//! session so the model can match a request against it, and loads the body
//! whenever the skill fires. An operator with ten saved workflows was spending
//! thousands of tokens before typing anything.
//!
//! So the template says each thing exactly once, in the fewest tokens that still
//! say it, and nothing that is true of every workflow is repeated in prose that
//! the reader could infer. Two rules keep it that way:
//!
//! - **Nothing is dropped, only relocated.** A description or an input note too
//!   long for the frontmatter is condensed there and kept whole in the body;
//!   what the body condenses stays in the store, one
//!   [`workflow_get`](GET_WORKFLOW_TOOL) call away, and the body says so. The
//!   skill is an index into the workflow, not a copy of it.
//! - **Condensing is sentence-aware.** A note is cut at a sentence boundary
//!   where one fits and at a word boundary otherwise, so a truncated line still
//!   reads as a sentence rather than as damage.

use super::marker::seal;
use super::render_helpers::{shell_quote_arg, yaml_scalar};
use super::{SkillInput, SkillWorkflow};
use serde_json::{json, Map, Value};

/// The tool a generated skill instructs the model to call.
const RUN_TOOL: &str = "mcp__medulla__workflow_run";

/// The tool that answers what the started run did.
///
/// Named in the body because starting a run and reading it are now two calls: a
/// skill that only knew the first would report "it is running" and never say
/// how it went.
const GET_TOOL: &str = "mcp__medulla__workflow_run_get";

/// How a session says which checkout the run should work in.
///
/// Written into every generated skill because the alternative is worse than not
/// knowing: a session that cannot say where a run works either starts it against
/// whatever directory the server happens to be in, or invents a declared input
/// for the path — which the script policy then refuses for leaving the
/// workspace.
/// Kept tight on purpose: this rides in every generated skill body, which is
/// held to a token budget the render tests enforce.
const WORKSPACE_SECTION: &str = "## Where it runs\n\n\
     The run works in the directory Medulla's MCP server started in. To point it \
     at another checkout, add `\"workspace\": \"<path>\"` to the call — absolute, or \
     relative to that one. That is the only way to move a run; a path passed as \
     an ordinary input cannot leave the workspace, and a workspace that is not a \
     directory is refused.\n";
/// The tool that serves the full text this rendering condensed.
///
/// Named wherever a note is shortened, so the shortening is a pointer rather
/// than a loss: the store still holds every word the workflow's author wrote.
const GET_WORKFLOW_TOOL: &str = "mcp__medulla__workflow_get";

/// How much workflow description the frontmatter carries.
///
/// The frontmatter is the part loaded into every session whether the skill fires
/// or not, so it is the one place where length is charged unconditionally. A
/// couple of sentences is enough for the model to match a paraphrased request
/// against; the rest lands in the body, which is only read once the match has
/// already been made.
pub(super) const MAX_FRONTMATTER_DESCRIPTION_CHARS: usize = 220;

/// How much of an input's note the inputs list carries.
///
/// Long enough for the sentence that says what the input *is*, short enough that
/// a workflow with a thoroughly documented signature does not turn its skill
/// into the documentation. The remainder stays in the store.
const MAX_INPUT_NOTE_CHARS: usize = 120;

/// The longest slug either verified harness accepts as a skill name.
///
/// Codex rejects a name over 64 characters outright (`InvalidField`), and the
/// name it validates is the directory basename when the frontmatter omits one.
/// Truncating here rather than letting the harness refuse keeps the failure
/// visible in the path the operator can see, instead of in a startup warning
/// they will not read.
const MAX_SLUG_CHARS: usize = 64;

/// The namespace every generated slug opens with.
///
/// Load-bearing beyond the cosmetic: [`super::install`] treats a `medulla-`
/// skill directory with no workflow behind it as ours to retire, so the prefix
/// is the one thing that makes a marker-less leftover recognisable.
pub(crate) const SLUG_PREFIX: &str = "medulla-";

/// The slug a workflow's skill is installed under.
///
/// Prefixed with `medulla-` so a directory listing of `~/.claude/skills` says
/// where these came from, and sanitised to the `[a-z0-9-]` alphabet that both
/// verified harnesses accept as a skill name — Codex additionally tokenises
/// `$name` mentions over `[A-Za-z0-9_\-:]`, so a slug carrying a dot or a space
/// could never be mentioned at all. An id that sanitises to nothing (all
/// punctuation) still yields a usable, if opaque, `medulla-workflow`.
pub fn slug_for(workflow_id: &str) -> String {
    let mut slug = String::with_capacity(workflow_id.len() + 8);
    let mut pending_dash = false;
    for ch in workflow_id.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(ch.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    if slug.is_empty() {
        slug.push_str("workflow");
    }
    let mut slug = format!("{SLUG_PREFIX}{slug}");
    if slug.len() > MAX_SLUG_CHARS {
        // ASCII by construction, so a byte truncation is a char truncation. A
        // trailing dash would read as an accident.
        slug.truncate(MAX_SLUG_CHARS);
        while slug.ends_with('-') {
            slug.pop();
        }
    }
    slug
}

/// Renders the `SKILL.md` for one workflow.
///
/// The returned [`RenderedSkill::body`] is the complete file: frontmatter whose
/// first entry is the managed marker, then the instructions. `rev` fingerprints
/// the generated content, so it changes if and only if that content does —
/// which is what makes an install idempotent across releases that do not touch
/// the template.
pub fn render(summary: &SkillWorkflow) -> super::RenderedSkill {
    let slug = slug_for(&summary.id);
    let description = description_for(summary);
    let content = skill_content(summary, &slug, &description);
    let (body, rev) = seal(&summary.id, &content);
    super::RenderedSkill {
        workflow_id: summary.id.clone(),
        slug,
        description,
        body,
        rev,
    }
}

/// Renders the slash-command variant of an already-rendered skill.
///
/// The command exists for the operator who would rather type `/medulla-babysit
/// 123` than describe the work; the model-facing instructions are the same call
/// with the typed text handed over as `$ARGUMENTS`. Returns the complete file,
/// marker included, so it follows the same managed-file discipline.
pub fn render_command(skill: &super::RenderedSkill, summary: &SkillWorkflow) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!(
        "description: {}\n",
        yaml_scalar(&skill.description)
    ));
    out.push_str(&format!(
        "argument-hint: {}\n",
        yaml_scalar(&argument_hint(&summary.inputs))
    ));
    out.push_str("---\n\n");
    out.push_str(&format!(
        "Run the `{id}` Medulla workflow for the operator, who typed: $ARGUMENTS\n\n\
         ```json\n{example}\n```\n\n",
        id = summary.id,
        example = call_example(summary)
    ));
    out.push_str(&inputs_section(summary));
    // The command is typed by an operator who wants it *run*, so it says the one
    // thing the skill's fallback line does not: what to do with their words.
    //
    // `$ARGUMENTS` now rides in the header line above, so the separate paragraph
    // this branch used to add here would say it twice.
    out.push('\n');
    out.push_str(WORKSPACE_SECTION);
    out.push_str(&format!(
        "\nMap the typed text onto those inputs, then follow the run with `{GET_TOOL}`.\n\n{}",
        fallback_line(summary)
    ));
    seal(&summary.id, &out).0
}

/// The frontmatter description: the workflow's own words plus a trigger clause.
///
/// The clause is not decoration. A description that only restates the workflow
/// name gives the model nothing to match a paraphrased request against, and the
/// whole feature turns on that match. It is also why the result is never empty:
/// a workflow whose author wrote no description still gets a usable trigger.
///
/// Both halves are kept short because this string is loaded into every session,
/// fired or not. The clause names the workflow id and the two things a request
/// can look like — "run it" or "do what it does" — in one line rather than a
/// sentence of throat-clearing, and the workflow's own words are condensed to
/// [`MAX_FRONTMATTER_DESCRIPTION_CHARS`]. Anything condensed away is restored in
/// the body by [`skill_content`], so the full text is still one skill-fire away.
fn description_for(summary: &SkillWorkflow) -> String {
    let trigger = format!(
        "Medulla workflow \"{id}\" — use when asked to run it, or to do this work.",
        id = summary.id
    );
    match condense(&summary.description, MAX_FRONTMATTER_DESCRIPTION_CHARS) {
        Some(own) => format!("{own} {trigger}"),
        None => trigger,
    }
}

/// Whether the frontmatter description dropped any of the workflow's own words.
///
/// Drives the one conditional paragraph in the body: reprinting a description
/// the frontmatter already carries in full would be the exact duplication this
/// template exists to remove, but silently losing the tail of a long one would
/// be worse.
fn description_was_condensed(summary: &SkillWorkflow) -> bool {
    was_condensed(&summary.description, MAX_FRONTMATTER_DESCRIPTION_CHARS)
}

/// Collapses all whitespace runs — newlines included — to single spaces.
///
/// Descriptions and input notes are authored as prose and may wrap over several
/// lines; every place they are rendered here is a one-line context (a YAML
/// scalar, a list item), so the normalisation happens once, up front.
fn normalise_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether rendering prose at `max` characters omits any of its words.
///
/// [`condense`] closes a short unterminated sentence with punctuation, which is
/// presentation rather than truncation. Track omission from the normalised
/// source length so that punctuation alone does not advertise a needless
/// workflow lookup.
fn was_condensed(text: &str, max: usize) -> bool {
    normalise_ws(text).chars().count() > max
}

/// Shortens prose to `max` characters, or `None` when there is no prose at all.
///
/// Sentence boundaries are preferred: a note cut after its first sentence still
/// reads as something its author wrote, whereas a note cut mid-clause reads as a
/// bug. Only when even the first sentence overruns does this fall back to a word
/// boundary and an ellipsis, which is the signal to the reader that the rest is
/// elsewhere. Text that already fits is returned whole with a closing period, so
/// the caller can always join it to a following sentence.
pub(super) fn condense(text: &str, max: usize) -> Option<String> {
    let text = normalise_ws(text);
    if text.is_empty() {
        return None;
    }
    if text.chars().count() <= max {
        return Some(terminate(&text));
    }
    // Accumulate whole sentences while they fit, so a two-sentence note under
    // the cap keeps both rather than only its first.
    let mut kept = String::new();
    for sentence in sentences(&text) {
        if kept.chars().count() + sentence.trim_end().chars().count() > max {
            break;
        }
        kept.push_str(sentence);
    }
    let kept = kept.trim_end();
    if !kept.is_empty() {
        return Some(terminate(kept));
    }
    // The opening sentence alone overruns: cut on a word boundary instead, and
    // say so with an ellipsis rather than pretending the sentence ended there.
    let mut truncated = String::new();
    for word in text.split(' ') {
        if truncated.chars().count() + word.chars().count() + 1 > max.saturating_sub(1) {
            break;
        }
        if !truncated.is_empty() {
            truncated.push(' ');
        }
        truncated.push_str(word);
    }
    if truncated.is_empty() {
        // A single word longer than the cap. Cut it rather than return nothing.
        truncated = text.chars().take(max.saturating_sub(1)).collect();
    }
    Some(format!("{truncated}…"))
}

/// Splits normalised prose into sentences, each keeping its own terminator and
/// the space that followed it.
///
/// Deliberately simple — a terminator followed by a space ends a sentence. An
/// abbreviation would split early, which costs a few characters of a condensed
/// note and nothing else; the alternative is a sentence tokeniser this file has
/// no business carrying.
fn sentences(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(byte, b'.' | b'!' | b'?') && bytes.get(index + 1) == Some(&b' ') {
            out.push(&text[start..index + 2]);
            start = index + 2;
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// Gives prose a closing period unless it already ends in terminating
/// punctuation, so it can be joined to the trigger clause without running on.
fn terminate(text: &str) -> String {
    if text.ends_with('.') || text.ends_with('!') || text.ends_with('?') || text.ends_with('…') {
        text.to_string()
    } else {
        format!("{text}.")
    }
}

/// The whole skill body below the marker.
///
/// Four things, in the order a model needs them: the call to make, the inputs to
/// fill in, what the answer means, and what to do when the tool is missing. The
/// headings that used to separate them are gone — a section header costs tokens
/// on every load and earns them back only in a document long enough to navigate,
/// which this deliberately is not.
fn skill_content(summary: &SkillWorkflow, slug: &str, description: &str) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("name: {slug}\n"));
    out.push_str(&format!("description: {}\n", yaml_scalar(description)));
    out.push_str("---\n\n");

    out.push_str(&format!(
        "# `{id}`\n\n```json\n{example}\n```\n\n",
        id = summary.id,
        example = call_example(summary)
    ));

    out.push_str(&inputs_section(summary));
    out.push('\n');
    out.push_str(WORKSPACE_SECTION);
    out.push('\n');

    out.push_str(&fallback_line(summary));
    out
}

/// The one sentence that keeps a skill useful when the server is not attached.
///
/// Everything else a caller needs about `workflow_run` — that it answers at once
/// with a `runId`, that the run outlives the call by minutes to hours, that
/// [`workflow_run_get`](GET_TOOL) is how you read it, that a missing or misnamed
/// input is rejected — is already in the tool's own MCP description and input
/// schema, which the model is holding whenever the tool exists. Restating it
/// here charged every skill for text the reader already had.
///
/// What the tool's description cannot cover is its own absence. Skills are
/// copied into user-scope directories that outlive any particular MCP
/// configuration, so a missing server is a normal state rather than a bug — and
/// a model that dead-ends there, or worse reports a run it never started, is
/// worse than no skill at all. Hence this line, and only this line.
fn fallback_line(summary: &SkillWorkflow) -> String {
    let inputs =
        serde_json::to_string(&example_input_map(summary)).unwrap_or_else(|_| "{}".to_string());
    format!(
        "No `{RUN_TOOL}`? The Medulla MCP server is not attached — say so rather than \
         claiming a start, and run:\n\n```sh\nmedulla workflow run {id} --inputs {inputs}\n```\n\n\
         Or attach it once with `medulla skills install --with-mcp`.\n",
        id = shell_quote_arg(&summary.id),
        inputs = shell_quote_arg(&inputs),
    )
}

/// The inputs list, or the sentence that replaces it when there are none.
///
/// A list rather than the markdown table this used to be: the table spent five
/// header cells and a rule row on every workflow, plus two pipes and padding per
/// input, to lay out what `name type = default — note` says in one line.
///
/// Notes are condensed to [`MAX_INPUT_NOTE_CHARS`]. Whenever this rendering — or
/// the frontmatter above it — had to shorten the author's words, the trailer
/// names [`workflow_get`](GET_WORKFLOW_TOOL), which serves them whole. That
/// pointer is why the skill can afford to be an index rather than a copy: the
/// full text is one call away, and only fetched by a model that needs it.
fn inputs_section(summary: &SkillWorkflow) -> String {
    let mut condensed_any = description_was_condensed(summary);
    if summary.inputs.is_empty() {
        let mut out = String::from("No inputs — pass `\"inputs\": {}`.\n");
        if condensed_any {
            out.push_str(&format!("\n{}\n", full_text_pointer().trim()));
        }
        return out;
    }
    let mut out = String::from("Inputs (`*` required):\n\n");
    for input in &summary.inputs {
        out.push_str(&format!(
            "- `{name}`{star} {ty}",
            name = input.name,
            star = if input.required { "*" } else { "" },
            ty = input.ty,
        ));
        if let Some(default) = &input.default {
            out.push_str(&format!(" = `{default}`"));
        }
        let note = input.description.as_deref().unwrap_or_default();
        if let Some(condensed) = condense(note, MAX_INPUT_NOTE_CHARS) {
            condensed_any |= was_condensed(note, MAX_INPUT_NOTE_CHARS);
            out.push_str(&format!(" — {condensed}"));
        }
        out.push('\n');
    }
    // Examples include type-shaped values for every input, including required
    // ones. Make clear that those placeholders demonstrate the call shape; they
    // are not authorisation to invent an operator's required values.
    if summary.inputs.iter().any(|input| input.required) {
        out.push_str(
            "\nBefore calling, ask the operator for every required `*` input they did not supply; never use an example placeholder as its value.\n",
        );
    }
    if condensed_any {
        out.push_str(&format!("\n{}\n", full_text_pointer().trim()));
    }
    out
}

/// The sentence that says where the words this rendering shortened still live.
fn full_text_pointer() -> String {
    format!("`{GET_WORKFLOW_TOOL}` has the full description and input notes.")
}

/// A concrete, copyable call for this workflow's declared signature.
///
/// Written on one line rather than pretty-printed: a model reads the argument
/// shape either way, and the indented form spends a line and its padding on
/// every input the list below already names.
fn call_example(summary: &SkillWorkflow) -> String {
    let call = json!({ "id": summary.id, "inputs": example_input_map(summary) });
    format!(
        "{RUN_TOOL}\n{}",
        serde_json::to_string(&call).unwrap_or_else(|_| "{}".to_string())
    )
}

/// Placeholder values for every declared input.
///
/// An input with a default shows its default (so the example is runnable as
/// written); everything else shows a type-shaped placeholder the model is meant
/// to replace, not a plausible-looking invented value.
fn example_input_map(summary: &SkillWorkflow) -> Map<String, Value> {
    let mut map = Map::new();
    for input in &summary.inputs {
        let value = match (&input.default, input.ty.as_str()) {
            (Some(default), _) => default.clone(),
            (None, "string") => Value::String(format!("<{}>", input.name)),
            (None, "number") => json!(0),
            (None, "boolean") => json!(false),
            (None, _) => json!({}),
        };
        map.insert(input.name.clone(), value);
    }
    map
}

/// The `argument-hint` line: required inputs in angle brackets, optional in
/// square ones, matching what both harnesses show beside a slash command.
fn argument_hint(inputs: &[SkillInput]) -> String {
    if inputs.is_empty() {
        return "(no inputs)".to_string();
    }
    inputs
        .iter()
        .map(|input| {
            if input.required {
                format!("<{}>", input.name)
            } else {
                format!("[{}]", input.name)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
