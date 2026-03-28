use std::io::{self, BufRead, Write};
use std::sync::mpsc;
use std::thread;

// ── ANSI colors ───────────────────────────────────────────────────────────────

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";

// Foreground
const FG_WHITE: &str = "\x1b[97m";
const FG_BLACK: &str = "\x1b[30m";

// Backgrounds — each section of the header bar gets its own bg
const BG_BLUE: &str = "\x1b[44m"; // 🔵 before: crumb+span
const BG_GREEN: &str = "\x1b[42m"; // 🟢 after:  crumb+span  /  agent-done key
const BG_YELLOW: &str = "\x1b[43m"; // 🏁 agent-done: crumb+span
const BG_CYAN: &str = "\x1b[46m"; // model name
const BG_MAGENTA: &str = "\x1b[45m"; // token counts
const BG_DARK: &str = "\x1b[100m"; // duration / step (bright-black bg)

/// Decision returned by `DebugHook::before_call`.
pub enum PreCallDecision {
    /// Proceed with this system prompt (None = use original).
    Proceed { system_override: Option<String> },
    /// Abort the entire run.
    Quit,
}

/// Decision returned by `DebugHook::after_agent`.
pub enum AfterAgentDecision {
    /// Accept the output and continue.
    Proceed,
    /// Restart the entire agent from scratch.
    Retry,
}

/// Hook called before/after every LLM call and after every agent completes.
/// Implementations must be `Send + Sync` so the Arc can be shared across async tasks.
pub trait DebugHook: Send + Sync {
    fn before_call(
        &self,
        agent: &str,
        step: &str,
        model: &str,
        system: &str,
        context: &str,
        trace_id: &str,
        span_id: &str,
        crumb: &str,
    ) -> PreCallDecision;

    fn after_call(
        &self,
        agent: &str,
        step: &str,
        response: &str,
        input_tokens: u32,
        output_tokens: u32,
        duration_ms: u128,
        trace_id: &str,
        span_id: &str,
        crumb: &str,
    );

    fn after_agent(
        &self,
        agent: &str,
        pattern: &str,
        key: &str,
        value: &str,
        trace_id: &str,
        span_id: &str,
        crumb: &str,
    ) -> AfterAgentDecision;
}

// ── NoopHook ──────────────────────────────────────────────────────────────────

pub struct NoopHook;

impl DebugHook for NoopHook {
    fn before_call(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> PreCallDecision {
        PreCallDecision::Proceed {
            system_override: None,
        }
    }
    fn after_call(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: u32,
        _: u32,
        _: u128,
        _: &str,
        _: &str,
        _: &str,
    ) {
    }
    fn after_agent(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> AfterAgentDecision {
        AfterAgentDecision::Proceed
    }
}

// ── CliDebugger ───────────────────────────────────────────────────────────────

enum DebugRequest {
    BeforeCall {
        agent: String,
        step: String,
        model: String,
        system: String,
        context: String,
        trace_id: String,
        span_id: String,
        crumb: String,
        respond: mpsc::SyncSender<BeforeCallResponse>,
    },
    AfterCall {
        agent: String,
        step: String,
        response_text: String,
        input_tokens: u32,
        output_tokens: u32,
        duration_ms: u128,
        trace_id: String,
        span_id: String,
        crumb: String,
        respond: mpsc::SyncSender<()>,
    },
    AfterAgent {
        agent: String,
        pattern: String,
        key: String,
        value: String,
        trace_id: String,
        span_id: String,
        crumb: String,
        respond: mpsc::SyncSender<AfterAgentResponse>,
    },
}

enum BeforeCallResponse {
    Proceed { system_override: Option<String> },
    Quit,
}

enum AfterAgentResponse {
    Proceed,
    Retry,
}

/// Interactive step-through debugger. A single background thread owns stdin.
/// Parallel workers send requests onto the same channel; handled one at a time.
pub struct CliDebugger {
    breakpoints: Vec<String>,
    tx: mpsc::Sender<DebugRequest>,
}

impl CliDebugger {
    pub fn new(breakpoints: Vec<String>) -> Self {
        let (tx, rx) = mpsc::channel::<DebugRequest>();
        thread::spawn(move || stdin_handler(rx));
        CliDebugger { breakpoints, tx }
    }

    fn should_pause(&self, agent: &str) -> bool {
        self.breakpoints.is_empty() || self.breakpoints.iter().any(|b| b == agent)
    }
}

impl DebugHook for CliDebugger {
    fn before_call(
        &self,
        agent: &str,
        step: &str,
        model: &str,
        system: &str,
        context: &str,
        trace_id: &str,
        span_id: &str,
        crumb: &str,
    ) -> PreCallDecision {
        if !self.should_pause(agent) {
            return PreCallDecision::Proceed {
                system_override: None,
            };
        }
        let (respond_tx, respond_rx) = mpsc::sync_channel(1);
        let _ = self.tx.send(DebugRequest::BeforeCall {
            agent: agent.to_string(),
            step: step.to_string(),
            model: model.to_string(),
            system: system.to_string(),
            context: context.to_string(),
            trace_id: trace_id.to_string(),
            span_id: span_id.to_string(),
            crumb: crumb.to_string(),
            respond: respond_tx,
        });
        let response = tokio::task::block_in_place(|| respond_rx.recv());
        match response {
            Ok(BeforeCallResponse::Proceed { system_override }) => {
                PreCallDecision::Proceed { system_override }
            }
            _ => PreCallDecision::Quit,
        }
    }

    fn after_call(
        &self,
        agent: &str,
        step: &str,
        response: &str,
        input_tokens: u32,
        output_tokens: u32,
        duration_ms: u128,
        trace_id: &str,
        span_id: &str,
        crumb: &str,
    ) {
        if !self.should_pause(agent) {
            return;
        }
        let (respond_tx, respond_rx) = mpsc::sync_channel(1);
        let _ = self.tx.send(DebugRequest::AfterCall {
            agent: agent.to_string(),
            step: step.to_string(),
            response_text: response.to_string(),
            input_tokens,
            output_tokens,
            duration_ms,
            trace_id: trace_id.to_string(),
            span_id: span_id.to_string(),
            crumb: crumb.to_string(),
            respond: respond_tx,
        });
        tokio::task::block_in_place(|| {
            let _ = respond_rx.recv();
        });
    }

    fn after_agent(
        &self,
        agent: &str,
        pattern: &str,
        key: &str,
        value: &str,
        trace_id: &str,
        span_id: &str,
        crumb: &str,
    ) -> AfterAgentDecision {
        if !self.should_pause(agent) {
            return AfterAgentDecision::Proceed;
        }
        let (respond_tx, respond_rx) = mpsc::sync_channel(1);
        let _ = self.tx.send(DebugRequest::AfterAgent {
            agent: agent.to_string(),
            pattern: pattern.to_string(),
            key: key.to_string(),
            value: value.to_string(),
            trace_id: trace_id.to_string(),
            span_id: span_id.to_string(),
            crumb: crumb.to_string(),
            respond: respond_tx,
        });
        let response = tokio::task::block_in_place(|| respond_rx.recv());
        match response {
            Ok(AfterAgentResponse::Retry) => AfterAgentDecision::Retry,
            _ => AfterAgentDecision::Proceed,
        }
    }
}

// ── stdin handler thread ───────────────────────────────────────────────────────

fn stdin_handler(rx: mpsc::Receiver<DebugRequest>) {
    for req in rx {
        match req {
            DebugRequest::BeforeCall {
                agent,
                step,
                model,
                system,
                context,
                trace_id,
                span_id,
                crumb,
                respond,
            } => {
                let r = handle_before_call(
                    &agent, &step, &model, &system, &context, &trace_id, &span_id, &crumb,
                );
                let _ = respond.send(r);
            }
            DebugRequest::AfterCall {
                agent,
                step,
                response_text,
                input_tokens,
                output_tokens,
                duration_ms,
                trace_id,
                span_id,
                crumb,
                respond,
            } => {
                handle_after_call(
                    &agent,
                    &step,
                    &response_text,
                    input_tokens,
                    output_tokens,
                    duration_ms,
                    &trace_id,
                    &span_id,
                    &crumb,
                );
                let _ = respond.send(());
            }
            DebugRequest::AfterAgent {
                agent,
                pattern,
                key,
                value,
                trace_id,
                span_id,
                crumb,
                respond,
            } => {
                let r =
                    handle_after_agent(&agent, &pattern, &key, &value, &trace_id, &span_id, &crumb);
                let _ = respond.send(r);
            }
        }
    }
}

// ── display handlers ──────────────────────────────────────────────────────────

fn handle_before_call(
    _agent: &str,
    step: &str,
    model: &str,
    system: &str,
    context: &str,
    trace_id: &str,
    span_id: &str,
    crumb: &str,
) -> BeforeCallResponse {
    let _ = trace_id; // not displayed
    let is_first = step.is_empty() || step.ends_with("_1");

    eprintln!();
    eprintln!(
        "{BG_BLUE}{FG_WHITE} {crumb}{DIM}<{}>{RESET}{BG_CYAN}{FG_BLACK} {model} {RESET}",
        short_id(span_id)
    );

    if is_first {
        eprintln!("{DIM}system ({} chars):{RESET}", system.len());
        eprintln!("{}", truncate(system, 600));
    } else if !context.is_empty() {
        eprintln!("{DIM}tool results:{RESET}");
        eprintln!("{}", truncate(context, 800));
    }
    eprintln!();

    loop {
        eprint!("{DIM}[Enter]{RESET} send  {DIM}[e]{RESET} edit  {DIM}[f]{RESET} full system  {DIM}[q]{RESET} quit > ");
        io::stderr().flush().ok();

        match read_line().trim() {
            "" => {
                return BeforeCallResponse::Proceed {
                    system_override: None,
                }
            }
            "f" => {
                eprintln!("\n{DIM}── full system ──{RESET}\n{system}\n{DIM}────────────────{RESET}")
            }
            "e" => {
                let new = edit_multiline("Enter new system prompt (end with '.' on its own line):");
                if new.is_empty() {
                    eprintln!("{DIM}(empty — keeping original){RESET}");
                } else {
                    eprintln!("{DIM}System prompt updated ({} chars).{RESET}", new.len());
                    return BeforeCallResponse::Proceed {
                        system_override: Some(new),
                    };
                }
            }
            "q" => return BeforeCallResponse::Quit,
            other => eprintln!("  unknown: '{other}'"),
        }
    }
}

fn handle_after_call(
    agent: &str,
    step: &str,
    response: &str,
    input_tokens: u32,
    output_tokens: u32,
    duration_ms: u128,
    trace_id: &str,
    span_id: &str,
    crumb: &str,
) {
    let _ = (agent, trace_id, step); // not displayed

    eprintln!();
    eprintln!("{BG_GREEN}{FG_BLACK} {crumb}{DIM}<{}>{RESET}{BG_MAGENTA}{FG_WHITE} ⬆ {input_tokens}  ⬇ {output_tokens}{RESET}{BG_DARK}{FG_WHITE} {duration_ms}ms {RESET}",
        short_id(span_id));
    eprintln!("{}", truncate(response, 800));
    eprintln!();

    loop {
        eprint!("{DIM}[Enter]{RESET} next  {DIM}[f]{RESET} full response  {DIM}[q]{RESET} quit > ");
        io::stderr().flush().ok();

        match read_line().trim() {
            "" => return,
            "f" => eprintln!(
                "\n{DIM}── full response ──{RESET}\n{response}\n{DIM}──────────────────{RESET}"
            ),
            "q" => {
                eprintln!("Aborting run.");
                std::process::exit(1);
            }
            other => eprintln!("  unknown: '{other}'"),
        }
    }
}

fn handle_after_agent(
    agent: &str,
    pattern: &str,
    key: &str,
    value: &str,
    trace_id: &str,
    span_id: &str,
    crumb: &str,
) -> AfterAgentResponse {
    let _ = (agent, trace_id); // not displayed

    eprintln!();
    eprintln!("{BG_YELLOW}{FG_BLACK} {crumb}{DIM}<{}>{RESET}{BG_CYAN}{FG_BLACK} {pattern} {RESET}{BG_GREEN}{FG_BLACK} → {key} {RESET}",
        short_id(span_id));
    eprintln!("{}", truncate(value, 800));
    eprintln!();

    loop {
        eprint!("{DIM}[Enter]{RESET} proceed  {DIM}[r]{RESET} retry  {DIM}[f]{RESET} full output  {DIM}[q]{RESET} quit > ");
        io::stderr().flush().ok();

        match read_line().trim() {
            "" => return AfterAgentResponse::Proceed,
            "r" => {
                eprintln!("  restarting agent '{agent}'...");
                return AfterAgentResponse::Retry;
            }
            "f" => {
                eprintln!("\n{DIM}── full output ──{RESET}\n{value}\n{DIM}────────────────{RESET}")
            }
            "q" => {
                eprintln!("Aborting run.");
                std::process::exit(1);
            }
            other => eprintln!("  unknown: '{other}'"),
        }
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// First 8 chars of a UUID — enough to visually identify a span.
fn short_id(id: &str) -> &str {
    &id[..id.len().min(8)]
}

fn read_line() -> String {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).ok();
    line
}

fn edit_multiline(prompt: &str) -> String {
    eprintln!("{prompt}");
    let mut lines = Vec::new();
    loop {
        eprint!("> ");
        io::stderr().flush().ok();
        let line = read_line();
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
        if trimmed == "." {
            break;
        }
        lines.push(trimmed.to_string());
    }
    lines.join("\n")
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}
