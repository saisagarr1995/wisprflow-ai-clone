//! Voice commands (design doc §10): transform the current selection (or last
//! dictation) via the LLM. A Tier-0 regex gate keeps plain dictation off the
//! LLM classification path entirely.

use crate::backend::{ChatMessage, LlmBackend};
use crate::guard::sanitize;
use crate::router::{model_for, TaskKind};
use openflow_settings::LlmConfig;

/// Where a command's output goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    /// Replace the selected text.
    Replace,
    /// Insert at the caret (no selection needed).
    Insert,
    /// Show in a popover; never inject (assistant answers).
    Popover,
}

/// A registered command: trigger phrases + a prompt template with `{input}`.
#[derive(Debug, Clone)]
pub struct Command {
    pub name: String,
    pub triggers: Vec<String>,
    pub prompt_template: String,
    pub task: TaskKind,
    pub output: OutputMode,
}

/// The built-in command set (design doc §10).
pub fn builtin_commands() -> Vec<Command> {
    let t = |s: &str| s.to_string();
    vec![
        Command {
            name: t("rewrite-professionally"),
            triggers: vec![t("rewrite professionally"), t("make it professional"), t("make this professional")],
            prompt_template: t("Rewrite the following text to be professional and polished. Output only the rewritten text:\n\n{input}"),
            task: TaskKind::Rewrite,
            output: OutputMode::Replace,
        },
        Command {
            name: t("summarize"),
            triggers: vec![t("summarize"), t("summarize this"), t("give me a summary")],
            prompt_template: t("Summarize the following text concisely. Output only the summary:\n\n{input}"),
            task: TaskKind::Rewrite,
            output: OutputMode::Replace,
        },
        Command {
            name: t("shorten"),
            triggers: vec![t("make it shorter"), t("shorten this"), t("make this shorter")],
            prompt_template: t("Make the following text shorter while keeping its meaning. Output only the result:\n\n{input}"),
            task: TaskKind::Rewrite,
            output: OutputMode::Replace,
        },
        Command {
            name: t("expand"),
            triggers: vec![t("expand this"), t("make it longer"), t("elaborate")],
            prompt_template: t("Expand the following text with more detail. Output only the result:\n\n{input}"),
            task: TaskKind::Rewrite,
            output: OutputMode::Replace,
        },
        Command {
            name: t("fix-grammar"),
            triggers: vec![t("fix grammar"), t("fix the grammar"), t("correct grammar")],
            prompt_template: t("Fix any spelling and grammar mistakes in the following text. Change nothing else. Output only the corrected text:\n\n{input}"),
            task: TaskKind::Cleanup,
            output: OutputMode::Replace,
        },
        Command {
            name: t("to-email"),
            triggers: vec![t("turn this into an email"), t("make this an email"), t("write as email")],
            prompt_template: t("Turn the following notes into a well-structured email. Output only the email:\n\n{input}"),
            task: TaskKind::Rewrite,
            output: OutputMode::Replace,
        },
        Command {
            name: t("to-bullets"),
            triggers: vec![t("make a bullet list"), t("turn into bullets"), t("create bullet list"), t("bullet list")],
            prompt_template: t("Convert the following text into a concise markdown bullet list. Output only the list:\n\n{input}"),
            task: TaskKind::Rewrite,
            output: OutputMode::Replace,
        },
        Command {
            name: t("to-markdown"),
            triggers: vec![t("convert to markdown"), t("make it markdown")],
            prompt_template: t("Format the following text as clean markdown. Output only the markdown:\n\n{input}"),
            task: TaskKind::Rewrite,
            output: OutputMode::Replace,
        },
        Command {
            name: t("translate"),
            triggers: vec![t("translate to"), t("translate this to")],
            prompt_template: t("Translate the following text to {arg}. Output only the translation:\n\n{input}"),
            task: TaskKind::Rewrite,
            output: OutputMode::Replace,
        },
        Command {
            name: t("explain"),
            triggers: vec![t("explain this"), t("what does this mean")],
            prompt_template: t("Explain the following clearly and concisely:\n\n{input}"),
            task: TaskKind::Assistant,
            output: OutputMode::Popover,
        },
        Command {
            name: t("ask"),
            triggers: vec![t("ask local ai"), t("ask ai"), t("hey ai")],
            prompt_template: t("{input}"),
            task: TaskKind::Assistant,
            output: OutputMode::Popover,
        },
    ]
}

/// A matched command plus the argument captured after the trigger (e.g. the
/// target language for "translate to German", or the question for "ask ai …").
pub struct CommandMatch<'a> {
    pub command: &'a Command,
    pub arg: Option<String>,
}

/// Tier-0 gate (design doc §10): match the utterance head against triggers.
/// `has_operand` is whether a selection or last-dictation exists; commands
/// that transform text require one, assistant commands do not.
pub fn match_command<'a>(
    utterance: &str,
    commands: &'a [Command],
    has_operand: bool,
) -> Option<CommandMatch<'a>> {
    let norm = normalize(utterance);
    // Longest trigger first, so "make this professional" beats "make".
    let mut best: Option<(&Command, &String)> = None;
    for cmd in commands {
        for trig in &cmd.triggers {
            let nt = normalize(trig);
            let is_match = norm == nt || norm.starts_with(&format!("{nt} "));
            let is_better = best
                .map(|(_, b)| nt.len() > normalize(b).len())
                .unwrap_or(true);
            if is_match && is_better {
                best = Some((cmd, trig));
            }
        }
    }
    let (command, trigger) = best?;
    // Transform commands are inert without something to operate on.
    if command.output != OutputMode::Popover && !has_operand {
        return None;
    }
    let arg = {
        let nt = normalize(trigger);
        let rest = norm.strip_prefix(&nt).map(str::trim).unwrap_or("");
        if rest.is_empty() {
            None
        } else {
            Some(rest.to_string())
        }
    };
    Some(CommandMatch { command, arg })
}

fn normalize(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Run a matched command. `input` is the operand (selection / last dictation,
/// or the raw arg for assistant questions).
pub fn run_command(
    backend: &dyn LlmBackend,
    m: &CommandMatch,
    input: &str,
    config: &LlmConfig,
) -> Result<String, crate::LlmError> {
    let prompt = m
        .command
        .prompt_template
        .replace("{arg}", m.arg.as_deref().unwrap_or(""))
        .replace("{input}", input);
    let model = model_for(m.command.task, config);
    let output = backend.chat(&model, &[ChatMessage::user(prompt)], false)?;
    // Assistant answers are shown verbatim; transforms are guarded.
    if m.command.output == OutputMode::Popover {
        Ok(output.trim().to_string())
    } else {
        Ok(sanitize(&output, input).unwrap_or_else(|_| output.trim().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::FakeLlm;

    #[test]
    fn matches_longest_trigger_and_requires_operand() {
        let cmds = builtin_commands();
        // With a selection, "make it shorter" matches.
        let m = match_command("make it shorter", &cmds, true).unwrap();
        assert_eq!(m.command.name, "shorten");
        // Without an operand, a transform command does not fire.
        assert!(match_command("make it shorter", &cmds, false).is_none());
    }

    #[test]
    fn assistant_commands_need_no_operand_and_capture_arg() {
        let cmds = builtin_commands();
        let m = match_command("ask local ai what is a monad", &cmds, false).unwrap();
        assert_eq!(m.command.name, "ask");
        assert_eq!(m.arg.as_deref(), Some("what is a monad"));
    }

    #[test]
    fn translate_captures_language() {
        let cmds = builtin_commands();
        let m = match_command("translate to german", &cmds, true).unwrap();
        assert_eq!(m.command.name, "translate");
        assert_eq!(m.arg.as_deref(), Some("german"));
    }

    #[test]
    fn plain_dictation_does_not_match() {
        let cmds = builtin_commands();
        assert!(match_command("the meeting is at three pm tomorrow", &cmds, true).is_none());
    }

    #[test]
    fn run_command_fills_template_and_guards() {
        let cmds = builtin_commands();
        let m = match_command("translate to french", &cmds, true).unwrap();
        let fake = FakeLlm::with_responses(["Bonjour le monde"]);
        let out = run_command(&fake, &m, "hello world", &LlmConfig::default()).unwrap();
        assert_eq!(out, "Bonjour le monde");
        let calls = fake.calls.lock().unwrap();
        // The captured arg ("french") is lowercased by normalization.
        assert!(calls[0].1[0].content.contains("french"));
        assert!(calls[0].1[0].content.contains("hello world"));
    }
}
