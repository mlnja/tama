use anyhow::{Context, Result};
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::sync::Arc;

use super::AgentOutput;
use crate::runtime::llm::LlmClient;
use crate::runtime::model_registry::ModelRegistry;
use crate::runtime::tracer::{TraceCtx, Tracer};

/// Human-in-the-loop: two-phase react with a stdin pause between phases.
///
/// Files:
///   AGENT.md body  → phase 1 system prompt
///   resume.md      → phase 2 system prompt
///
/// Flow:
///   Phase 1: react loop → agent calls finish(channel_id, context_for_human)
///   PAUSE:   runtime prints context, reads human response from stdin
///   Phase 2: react loop with resume.md → start() returns human response
pub async fn run(
    agent_dir: &Path,
    body: &str,
    uses: &[String],
    max_iter: u32,
    _registry: &Arc<ModelRegistry>,
    client: &LlmClient,
    input: &str,
    tracer: &mut dyn Tracer,
    ctx: &TraceCtx,
    crumb: &str,
) -> Result<AgentOutput> {
    eprintln!("  → human: phase 1");
    let phase1 = super::react::run(body, uses, max_iter, client, input, tracer, ctx, crumb).await?;

    let channel_id = &phase1.key;
    let context = &phase1.value;

    eprintln!();
    eprintln!("━━━ human input required ━━━");
    eprintln!("channel: {channel_id}");
    eprintln!();
    eprintln!("{context}");
    eprintln!();
    eprint!("> ");
    io::stderr().flush().context("failed to flush stderr")?;

    let mut human_response = String::new();
    io::stdin()
        .lock()
        .read_line(&mut human_response)
        .context("failed to read human input from stdin")?;
    let human_response = human_response.trim().to_string();
    eprintln!("━━━ resuming ━━━");
    eprintln!();

    let resume_system = read_prompt(agent_dir, "resume.md")?;
    eprintln!("  → human: phase 2");
    super::react::run(
        &resume_system,
        &[],
        max_iter,
        client,
        &human_response,
        tracer,
        ctx,
        crumb,
    )
    .await
}

fn read_prompt(agent_dir: &Path, filename: &str) -> Result<String> {
    let path = agent_dir.join(filename);
    std::fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))
}
