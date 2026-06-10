//! Built-in agent definitions. Each codon flow that called a model
//! directly (or seeded a prompt prefix into the agent panel) now
//! reads its configuration from one of these — never from inline
//! constants — so a `codon.toml` `[agent.<name>]` override moves the
//! needle in one place.
//!
//! Today's roster:
//!
//! - `fish_complete` — the `#@` shell-completion flow. Cheap +
//!   cache-friendly Haiku 4.5 by default, system prompt designed to
//!   keep the model from wrapping commands in fences.
//! - `explain` / `summarize` / `refactor` — the cross-pane verbs.
//!   `user_prefix` carries the seeded text that lands in the agent
//!   panel's message editor when the verb fires. Model / system
//!   prompt fields are present so future migration to a fully
//!   in-house chat surface stays a swap.

use crate::runtime::agent::Agent;
use crate::runtime::model::{ModelSpec, ZedModelClient};
use crate::runtime::registry::AgentRegistry;
use gpui::App;
use std::sync::Arc;

pub const FISH_COMPLETE: &str = "fish_complete";
pub const EXPLAIN: &str = "explain";
pub const SUMMARIZE: &str = "summarize";
pub const REFACTOR: &str = "refactor";

const FISH_SYSTEM_PROMPT: &str = "\
You translate a short natural-language description into a single \
fish-shell command line. Output ONLY the command — no fences, no \
prose, no leading prompt, no trailing newline. Use fish syntax \
(parentheses for command substitution, `set` for variables, no \
`$((...))` arithmetic). If a partial command is provided, extend it \
in place; do not repeat it.";

const EXPLAIN_PREFIX: &str = "Please explain this:\n\n";
const SUMMARIZE_PREFIX: &str = "Please summarize:\n\n";
const REFACTOR_PREFIX: &str = "Please refactor this code, keeping behavior identical:\n\n";

pub fn register_builtins(cx: &mut App) {
    AgentRegistry::register(cx, fish_complete());
    AgentRegistry::register(cx, explain());
    AgentRegistry::register(cx, summarize());
    AgentRegistry::register(cx, refactor());
}

fn fish_complete() -> Agent {
    Agent::builder(
        FISH_COMPLETE,
        Arc::new(ZedModelClient::new(ModelSpec::Bare(
            "claude-haiku-4-5".to_string(),
        ))),
    )
    .system_prompt(FISH_SYSTEM_PROMPT)
    .temperature(0.0)
    .max_turns(1)
    .cache_system_prompt(true)
    .build()
}

fn explain() -> Agent {
    Agent::builder(EXPLAIN, Arc::new(ZedModelClient::new(ModelSpec::Default)))
        .user_prefix(EXPLAIN_PREFIX)
        .build()
}

fn summarize() -> Agent {
    Agent::builder(SUMMARIZE, Arc::new(ZedModelClient::new(ModelSpec::Default)))
        .user_prefix(SUMMARIZE_PREFIX)
        .build()
}

fn refactor() -> Agent {
    Agent::builder(REFACTOR, Arc::new(ZedModelClient::new(ModelSpec::Default)))
        .user_prefix(REFACTOR_PREFIX)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refactor_prefix_pins_behavior_invariant() {
        assert!(REFACTOR_PREFIX.contains("keeping behavior identical"));
    }

    #[test]
    fn prefixes_are_distinct() {
        assert_ne!(EXPLAIN_PREFIX, SUMMARIZE_PREFIX);
        assert_ne!(EXPLAIN_PREFIX, REFACTOR_PREFIX);
        assert_ne!(SUMMARIZE_PREFIX, REFACTOR_PREFIX);
    }

    #[test]
    fn explain_prefix_ends_with_blank_line_separator() {
        assert!(EXPLAIN_PREFIX.ends_with("\n\n"));
        assert!(SUMMARIZE_PREFIX.ends_with("\n\n"));
        assert!(REFACTOR_PREFIX.ends_with("\n\n"));
    }
}
