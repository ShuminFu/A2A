//! A small text-tools agent, written to exercise every part of the runtime.

use std::time::Duration;

use a2a_server::{AgentExecutor, EventSender, RequestContext};
use a2a_types::{A2aError, AgentCapabilities, AgentCard, AgentInterface, AgentSkill, Artifact};

/// The text transformations this agent knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    Echo,
    Reverse,
    Uppercase,
    WordCount,
    /// A deliberately slow job, so cancellation has something to interrupt.
    Slow,
}

impl Tool {
    /// Splits a message into its leading command word and the text to work on. A message with
    /// no command word is all input, which is what a reply to an `INPUT_REQUIRED` prompt looks
    /// like.
    fn parse(text: &str) -> (Option<Tool>, String) {
        let trimmed = text.trim();
        let (head, rest) = trimmed
            .split_once(char::is_whitespace)
            .unwrap_or((trimmed, ""));
        let tool = match head.to_ascii_lowercase().as_str() {
            "echo" => Tool::Echo,
            "reverse" => Tool::Reverse,
            "upper" | "uppercase" => Tool::Uppercase,
            "count" | "wordcount" => Tool::WordCount,
            "slow" => Tool::Slow,
            _ => return (None, trimmed.to_string()),
        };
        (Some(tool), rest.trim().to_string())
    }

    fn apply(self, input: &str) -> String {
        match self {
            Tool::Echo => input.to_string(),
            Tool::Reverse => input.chars().rev().collect(),
            Tool::Uppercase => input.to_uppercase(),
            Tool::WordCount => format!("{} words", input.split_whitespace().count()),
            Tool::Slow => format!("finished slow work on {} characters", input.len()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Tool::Echo => "echo",
            Tool::Reverse => "reverse",
            Tool::Uppercase => "uppercase",
            Tool::WordCount => "wordcount",
            Tool::Slow => "slow",
        }
    }
}

/// The sample agent.
pub struct TextToolsAgent;

#[async_trait::async_trait]
impl AgentExecutor for TextToolsAgent {
    async fn execute(&self, mut ctx: RequestContext, events: EventSender) -> Result<(), A2aError> {
        let text = ctx.text();
        let (named_tool, input) = Tool::parse(&text);
        // A reply to an earlier prompt names no tool, so the tool comes from the turn that
        // asked. The task history is what carries that across the pause.
        let tool = named_tool
            .or_else(|| pending_tool(&ctx))
            .unwrap_or(Tool::Echo);

        // An empty payload pauses the task instead of failing it: the client answers with
        // another message carrying the same task id, and execution resumes from there.
        if input.is_empty() {
            events
                .input_required(format!(
                    "Which text should I {}? Reply with the text to continue.",
                    tool.name()
                ))
                .await;
            return Ok(());
        }

        events
            .working(format!(
                "running {} on {} characters",
                tool.name(),
                input.len()
            ))
            .await;

        if tool == Tool::Slow {
            for step in 1..=5 {
                // Cancellation is cooperative: the runtime flips the signal and the agent
                // decides where it is safe to stop.
                if ctx.cancel.is_canceled() {
                    return Ok(());
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(200)) => {}
                    _ = ctx.cancel.canceled() => return Ok(()),
                }
                events
                    .artifact_chunk(
                        Artifact::text("progress", "progress log", format!("step {step} of 5\n")),
                        step > 1,
                        step == 5,
                    )
                    .await;
            }
        }

        let output = tool.apply(&input);
        events
            .artifact(Artifact::text("result", tool.name(), output.clone()))
            .await;
        events.complete(output).await;
        Ok(())
    }

    async fn cancel(&self, task_id: &str) -> Result<(), A2aError> {
        tracing::info!(task = %task_id, "cancellation requested");
        Ok(())
    }
}

/// The tool an earlier turn of this task asked about but never received text for.
///
/// Only messages before the current one count: the current message is already in the history
/// by the time the runtime calls the agent.
fn pending_tool(ctx: &RequestContext) -> Option<Tool> {
    let history = &ctx.task.history;
    let earlier = history.len().saturating_sub(1);
    history[..earlier]
        .iter()
        .rev()
        .filter(|message| message.role == a2a_types::Role::User)
        .find_map(|message| Tool::parse(&message.text()).0)
}

/// The card this agent publishes at `/.well-known/agent-card.json`.
pub fn agent_card(public_url: &str) -> AgentCard {
    AgentCard {
        name: "Text Tools".to_string(),
        description: "Transforms text: echo, reverse, uppercase and word count.".to_string(),
        supported_interfaces: vec![AgentInterface::jsonrpc(format!(
            "{}{}",
            public_url.trim_end_matches('/'),
            a2a_types::JSONRPC_PATH
        ))],
        provider: None,
        version: env!("CARGO_PKG_VERSION").to_string(),
        documentation_url: None,
        capabilities: AgentCapabilities {
            streaming: Some(true),
            push_notifications: Some(true),
            extensions: Vec::new(),
            extended_agent_card: Some(true),
        },
        security_schemes: Default::default(),
        security_requirements: Vec::new(),
        default_input_modes: vec!["text/plain".to_string()],
        default_output_modes: vec!["text/plain".to_string()],
        skills: vec![
            AgentSkill {
                id: "echo".to_string(),
                name: "Echo".to_string(),
                description: "Returns the text it was given.".to_string(),
                tags: vec!["text".to_string()],
                examples: vec!["echo hello world".to_string()],
                input_modes: Vec::new(),
                output_modes: Vec::new(),
                security_requirements: Vec::new(),
            },
            AgentSkill {
                id: "reverse".to_string(),
                name: "Reverse".to_string(),
                description: "Reverses the characters of the text.".to_string(),
                tags: vec!["text".to_string()],
                examples: vec!["reverse hello".to_string()],
                input_modes: Vec::new(),
                output_modes: Vec::new(),
                security_requirements: Vec::new(),
            },
            AgentSkill {
                id: "wordcount".to_string(),
                name: "Word count".to_string(),
                description: "Counts the words in the text.".to_string(),
                tags: vec!["text".to_string(), "analysis".to_string()],
                examples: vec!["count one two three".to_string()],
                input_modes: Vec::new(),
                output_modes: Vec::new(),
                security_requirements: Vec::new(),
            },
            AgentSkill {
                id: "slow".to_string(),
                name: "Slow job".to_string(),
                description: "A multi-second job that streams progress and can be canceled."
                    .to_string(),
                tags: vec!["demo".to_string()],
                examples: vec!["slow a long document".to_string()],
                input_modes: Vec::new(),
                output_modes: Vec::new(),
                security_requirements: Vec::new(),
            },
        ],
        signatures: Vec::new(),
        icon_url: None,
    }
}

/// The card authenticated callers receive from `GetExtendedAgentCard`.
pub fn extended_agent_card(public_url: &str) -> AgentCard {
    let mut card = agent_card(public_url);
    card.description =
        "Transforms text: echo, reverse, uppercase, word count, and a slow demo job.".to_string();
    card.skills.push(AgentSkill {
        id: "uppercase".to_string(),
        name: "Uppercase".to_string(),
        description: "Upper-cases the text. Only advertised to authenticated clients.".to_string(),
        tags: vec!["text".to_string(), "private".to_string()],
        examples: vec!["upper hello".to_string()],
        input_modes: Vec::new(),
        output_modes: Vec::new(),
        security_requirements: Vec::new(),
    });
    card
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2a_types::{Message, Task};

    fn context(history: &[&str]) -> RequestContext {
        let mut task = Task::submitted("task-1", "ctx-1");
        task.history = history
            .iter()
            .enumerate()
            .map(|(index, text)| Message::user_text(format!("msg-{index}"), *text))
            .collect();
        RequestContext {
            message: task.history.last().cloned().expect("history is not empty"),
            task,
            tenant: None,
            cancel: a2a_server::CancelSignal::never_canceled(),
        }
    }

    #[test]
    fn a_command_word_selects_the_tool_and_the_rest_is_input() {
        assert_eq!(
            Tool::parse("reverse hello"),
            (Some(Tool::Reverse), "hello".to_string())
        );
        assert_eq!(
            Tool::parse("COUNT one two"),
            (Some(Tool::WordCount), "one two".to_string())
        );
        assert_eq!(Tool::parse("reverse"), (Some(Tool::Reverse), String::new()));
    }

    #[test]
    fn a_message_without_a_command_word_is_all_input() {
        assert_eq!(
            Tool::parse("hello there"),
            (None, "hello there".to_string())
        );
    }

    #[test]
    fn a_reply_to_a_prompt_uses_the_tool_the_prompt_asked_about() {
        // The turn that paused named `reverse` with nothing to work on; the reply is bare text.
        let ctx = context(&["reverse", "stressed"]);
        assert_eq!(pending_tool(&ctx), Some(Tool::Reverse));
        assert_eq!(Tool::Reverse.apply("stressed"), "desserts");
    }

    #[test]
    fn a_first_message_has_no_pending_tool() {
        assert_eq!(pending_tool(&context(&["hello"])), None);
    }

    #[test]
    fn tools_transform_text_as_advertised() {
        assert_eq!(Tool::Echo.apply("hi"), "hi");
        assert_eq!(Tool::Uppercase.apply("hi"), "HI");
        assert_eq!(Tool::WordCount.apply("one two three"), "3 words");
    }
}
