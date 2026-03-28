use uuid::Uuid;

// ── Context ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct TraceCtx {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
}

impl TraceCtx {
    pub fn new_root(trace_id: String) -> Self {
        TraceCtx {
            trace_id,
            span_id: new_span_id(),
            parent_span_id: None,
        }
    }

    pub fn child(&self) -> Self {
        TraceCtx {
            trace_id: self.trace_id.clone(),
            span_id: new_span_id(),
            parent_span_id: Some(self.span_id.clone()),
        }
    }
}

fn new_span_id() -> String {
    Uuid::new_v4().to_string()
}

pub fn new_node_id() -> String {
    Uuid::new_v4().to_string()
}

// ── Trait ─────────────────────────────────────────────────────────────────────

pub trait Tracer: Send {
    fn on_run_start(&mut self, ctx: &TraceCtx, entrypoint: &str, task: &str);
    fn on_run_end(&mut self, ctx: &TraceCtx, status: &str, output: &str, duration_ms: u128);
    /// `prev_span_id`: the span that completed immediately before this agent started,
    /// within the same parent. When `prev_span_id == parent_span_id` and multiple
    /// siblings share this, they are running in parallel.
    fn on_agent_start(
        &mut self,
        ctx: &TraceCtx,
        agent: &str,
        pattern: &str,
        input: &str,
        prev_span_id: Option<&str>,
        node_id: &str,
    );
    fn on_agent_end(&mut self, ctx: &TraceCtx, key: &str, output: &str, duration_ms: u128);
    fn on_llm_call(
        &mut self,
        ctx: &TraceCtx,
        step: &str,
        model: &str,
        role: &str,
        temperature: Option<f32>,
        system: &str,
        response: &str,
        input_tokens: u32,
        output_tokens: u32,
        duration_ms: u128,
    );
    fn on_tool_call(
        &mut self,
        ctx: &TraceCtx,
        tool: &str,
        args_json: &str,
        result: &str,
        duration_ms: u128,
    );
    /// Synthetic start: oneshot step — runtime passed input directly without a start() tool call.
    /// Stored with kind='synthetic' so the UI shows the input alongside the LLM call.
    fn on_synthetic_start(&mut self, ctx: &TraceCtx, input: &str);
    /// Synthetic finish: model returned plain text instead of calling finish(), or oneshot completed.
    /// Stored with kind='synthetic' so the UI can distinguish it from a real tool call.
    fn on_synthetic_finish(&mut self, ctx: &TraceCtx, args_json: &str, result: &str);
}

// ── NoopTracer ────────────────────────────────────────────────────────────────

pub struct NoopTracer;
impl Tracer for NoopTracer {
    fn on_run_start(&mut self, _: &TraceCtx, _: &str, _: &str) {}
    fn on_run_end(&mut self, _: &TraceCtx, _: &str, _: &str, _: u128) {}
    fn on_agent_start(
        &mut self,
        _: &TraceCtx,
        _: &str,
        _: &str,
        _: &str,
        _: Option<&str>,
        _: &str,
    ) {
    }
    fn on_agent_end(&mut self, _: &TraceCtx, _: &str, _: &str, _: u128) {}
    fn on_llm_call(
        &mut self,
        _: &TraceCtx,
        _: &str,
        _: &str,
        _: &str,
        _: Option<f32>,
        _: &str,
        _: &str,
        _: u32,
        _: u32,
        _: u128,
    ) {
    }
    fn on_tool_call(&mut self, _: &TraceCtx, _: &str, _: &str, _: &str, _: u128) {}
    fn on_synthetic_start(&mut self, _: &TraceCtx, _: &str) {}
    fn on_synthetic_finish(&mut self, _: &TraceCtx, _: &str, _: &str) {}
}

// ── OtelTracer ────────────────────────────────────────────────────────────────

pub struct OtelTracer {
    enabled: bool,
}

impl Default for OtelTracer {
    fn default() -> Self {
        Self::new()
    }
}

impl OtelTracer {
    pub fn new() -> Self {
        let enabled = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok()
            && std::env::var("OTEL_SDK_DISABLED")
                .map(|v| v != "true")
                .unwrap_or(true);
        if enabled {
            eprintln!("otel: tracing enabled — full wiring coming in Phase 3");
        }
        OtelTracer { enabled }
    }
}

impl Tracer for OtelTracer {
    fn on_run_start(&mut self, _: &TraceCtx, _: &str, _: &str) {}
    fn on_run_end(&mut self, _: &TraceCtx, _: &str, _: &str, _: u128) {}
    fn on_agent_start(
        &mut self,
        _: &TraceCtx,
        _: &str,
        _: &str,
        _: &str,
        _: Option<&str>,
        _: &str,
    ) {
    }
    fn on_agent_end(&mut self, _: &TraceCtx, _: &str, _: &str, _: u128) {}
    fn on_llm_call(
        &mut self,
        _: &TraceCtx,
        _: &str,
        _: &str,
        _: &str,
        _: Option<f32>,
        _: &str,
        _: &str,
        _: u32,
        _: u32,
        _: u128,
    ) {
    }
    fn on_tool_call(&mut self, _: &TraceCtx, _: &str, _: &str, _: &str, _: u128) {}
    fn on_synthetic_start(&mut self, _: &TraceCtx, _: &str) {}
    fn on_synthetic_finish(&mut self, _: &TraceCtx, _: &str, _: &str) {}
}

// ── CompositeTracer ───────────────────────────────────────────────────────────

pub struct CompositeTracer {
    tracers: Vec<Box<dyn Tracer>>,
}

impl CompositeTracer {
    pub fn new(tracers: Vec<Box<dyn Tracer>>) -> Self {
        CompositeTracer { tracers }
    }
}

impl Tracer for CompositeTracer {
    fn on_run_start(&mut self, ctx: &TraceCtx, e: &str, t: &str) {
        for t_ in &mut self.tracers {
            t_.on_run_start(ctx, e, t);
        }
    }
    fn on_run_end(&mut self, ctx: &TraceCtx, s: &str, o: &str, d: u128) {
        for t in &mut self.tracers {
            t.on_run_end(ctx, s, o, d);
        }
    }
    fn on_agent_start(
        &mut self,
        ctx: &TraceCtx,
        agent: &str,
        pattern: &str,
        input: &str,
        prev: Option<&str>,
        node_id: &str,
    ) {
        for t in &mut self.tracers {
            t.on_agent_start(ctx, agent, pattern, input, prev, node_id);
        }
    }
    fn on_agent_end(&mut self, ctx: &TraceCtx, key: &str, output: &str, duration_ms: u128) {
        for t in &mut self.tracers {
            t.on_agent_end(ctx, key, output, duration_ms);
        }
    }
    fn on_llm_call(
        &mut self,
        ctx: &TraceCtx,
        step: &str,
        model: &str,
        role: &str,
        temperature: Option<f32>,
        system: &str,
        response: &str,
        in_tok: u32,
        out_tok: u32,
        dur: u128,
    ) {
        for t in &mut self.tracers {
            t.on_llm_call(
                ctx,
                step,
                model,
                role,
                temperature,
                system,
                response,
                in_tok,
                out_tok,
                dur,
            );
        }
    }
    fn on_tool_call(&mut self, ctx: &TraceCtx, tool: &str, args: &str, result: &str, dur: u128) {
        for t in &mut self.tracers {
            t.on_tool_call(ctx, tool, args, result, dur);
        }
    }
    fn on_synthetic_start(&mut self, ctx: &TraceCtx, input: &str) {
        for t in &mut self.tracers {
            t.on_synthetic_start(ctx, input);
        }
    }
    fn on_synthetic_finish(&mut self, ctx: &TraceCtx, args_json: &str, result: &str) {
        for t in &mut self.tracers {
            t.on_synthetic_finish(ctx, args_json, result);
        }
    }
}

// ── BufferedTracer ────────────────────────────────────────────────────────────

pub struct BufferedTracer {
    events: Vec<BufferedEvent>,
}

enum BufferedEvent {
    AgentStart {
        ctx: TraceCtx,
        agent: String,
        pattern: String,
        input: String,
        prev_span_id: Option<String>,
        node_id: String,
    },
    AgentEnd {
        ctx: TraceCtx,
        key: String,
        output: String,
        duration_ms: u128,
    },
    LlmCall {
        ctx: TraceCtx,
        step: String,
        model: String,
        role: String,
        temperature: Option<f32>,
        system: String,
        response: String,
        input_tokens: u32,
        output_tokens: u32,
        duration_ms: u128,
    },
    ToolCall {
        ctx: TraceCtx,
        tool: String,
        args_json: String,
        result: String,
        duration_ms: u128,
    },
    SyntheticStart {
        ctx: TraceCtx,
        input: String,
    },
    SyntheticFinish {
        ctx: TraceCtx,
        args_json: String,
        result: String,
    },
}

impl Default for BufferedTracer {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferedTracer {
    pub fn new() -> Self {
        BufferedTracer { events: Vec::new() }
    }

    pub fn flush_into(self, tracer: &mut dyn Tracer) {
        for event in self.events {
            match event {
                BufferedEvent::AgentStart {
                    ctx,
                    agent,
                    pattern,
                    input,
                    prev_span_id,
                    node_id,
                } => tracer.on_agent_start(
                    &ctx,
                    &agent,
                    &pattern,
                    &input,
                    prev_span_id.as_deref(),
                    &node_id,
                ),
                BufferedEvent::AgentEnd {
                    ctx,
                    key,
                    output,
                    duration_ms,
                } => tracer.on_agent_end(&ctx, &key, &output, duration_ms),
                BufferedEvent::LlmCall {
                    ctx,
                    step,
                    model,
                    role,
                    temperature,
                    system,
                    response,
                    input_tokens,
                    output_tokens,
                    duration_ms,
                } => tracer.on_llm_call(
                    &ctx,
                    &step,
                    &model,
                    &role,
                    temperature,
                    &system,
                    &response,
                    input_tokens,
                    output_tokens,
                    duration_ms,
                ),
                BufferedEvent::ToolCall {
                    ctx,
                    tool,
                    args_json,
                    result,
                    duration_ms,
                } => tracer.on_tool_call(&ctx, &tool, &args_json, &result, duration_ms),
                BufferedEvent::SyntheticStart { ctx, input } => {
                    tracer.on_synthetic_start(&ctx, &input)
                }
                BufferedEvent::SyntheticFinish {
                    ctx,
                    args_json,
                    result,
                } => tracer.on_synthetic_finish(&ctx, &args_json, &result),
            }
        }
    }
}

impl Tracer for BufferedTracer {
    fn on_run_start(&mut self, _: &TraceCtx, _: &str, _: &str) {}
    fn on_run_end(&mut self, _: &TraceCtx, _: &str, _: &str, _: u128) {}
    fn on_agent_start(
        &mut self,
        ctx: &TraceCtx,
        agent: &str,
        pattern: &str,
        input: &str,
        prev_span_id: Option<&str>,
        node_id: &str,
    ) {
        self.events.push(BufferedEvent::AgentStart {
            ctx: ctx.clone(),
            agent: agent.to_string(),
            pattern: pattern.to_string(),
            input: input.to_string(),
            node_id: node_id.to_string(),
            prev_span_id: prev_span_id.map(|s| s.to_string()),
        });
    }
    fn on_agent_end(&mut self, ctx: &TraceCtx, key: &str, output: &str, duration_ms: u128) {
        self.events.push(BufferedEvent::AgentEnd {
            ctx: ctx.clone(),
            key: key.to_string(),
            output: output.to_string(),
            duration_ms,
        });
    }
    fn on_llm_call(
        &mut self,
        ctx: &TraceCtx,
        step: &str,
        model: &str,
        role: &str,
        temperature: Option<f32>,
        system: &str,
        response: &str,
        input_tokens: u32,
        output_tokens: u32,
        duration_ms: u128,
    ) {
        self.events.push(BufferedEvent::LlmCall {
            ctx: ctx.clone(),
            step: step.to_string(),
            model: model.to_string(),
            role: role.to_string(),
            temperature,
            system: system.to_string(),
            response: response.to_string(),
            input_tokens,
            output_tokens,
            duration_ms,
        });
    }
    fn on_tool_call(
        &mut self,
        ctx: &TraceCtx,
        tool: &str,
        args_json: &str,
        result: &str,
        duration_ms: u128,
    ) {
        self.events.push(BufferedEvent::ToolCall {
            ctx: ctx.clone(),
            tool: tool.to_string(),
            args_json: args_json.to_string(),
            result: result.to_string(),
            duration_ms,
        });
    }
    fn on_synthetic_start(&mut self, ctx: &TraceCtx, input: &str) {
        self.events.push(BufferedEvent::SyntheticStart {
            ctx: ctx.clone(),
            input: input.to_string(),
        });
    }
    fn on_synthetic_finish(&mut self, ctx: &TraceCtx, args_json: &str, result: &str) {
        self.events.push(BufferedEvent::SyntheticFinish {
            ctx: ctx.clone(),
            args_json: args_json.to_string(),
            result: result.to_string(),
        });
    }
}
