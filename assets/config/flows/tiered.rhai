// Tiered roster example flow (REQ:codon/agent-roster#c-example-flow).
//
// Orchestrator plans and dispatches; a cheap implementer executes; a
// deliberately stronger reviewer verifies (generator/verifier
// asymmetry); a temperature-0 classifier gates every shell command,
// with the reviewer as second opinion for deny escalation.
//
// To use: copy this file and prompts/ to ~/.config/codon/flows/, then
// set in codon.toml:
//   [agent_harness]
//   active_flow = "tiered"

provider("openrouter");

agent("orchestrator", #{
    model: "z-ai/glm-5.2",
    prompt_file: "prompts/orchestrator.md",
    max_turns: 24,
});

agent("implementer", #{
    model: "deepseek/deepseek-v4-flash",
    prompt_file: "prompts/implementer.md",
    tools: ["shell"],
    max_turns: 16,
});

agent("reviewer", #{
    model: "z-ai/glm-5.2",
    prompt_file: "prompts/reviewer.md",
    tools: ["shell"],
    max_turns: 12,
});

agent("safety", #{
    model: "deepseek/deepseek-v4-flash",
    prompt_file: "prompts/safety.md",
    temperature: 0.0,
    max_turns: 1,
});

handoff("orchestrator", "implementer", #{
    name: "implement_task",
    description: "Dispatch ONE well-specified implementation task. Include: Goal, Files, Change, Constraints, Done-criteria, task id (Tn), criticality, and the verbatim done-bar from your instructions.",
});

handoff("orchestrator", "reviewer", #{
    name: "review_task",
    description: "Verify implementer work. Include: the user's original request verbatim, the dispatched spec, and the implementer's status block.",
});

safety_for("shell", "safety", "reviewer");
entrypoint("orchestrator");
