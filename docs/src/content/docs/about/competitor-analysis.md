---
title: Competitor Analysis
description: How tama compares to Docker Agent, Julep, LangGraph, CrewAI, Agno, and WSO2 AFM — named reasoning patterns, FSM routing, and zero-code agent composition.
---

*Last updated: 2026-03-28*

tama occupies a real gap: **named reasoning patterns as a first-class declaration, in plain Markdown files, executed by a local Rust binary.** No single competitor does all three.

| | tama | Docker Agent | Julep | LangGraph | CrewAI | Agno | WSO2 AFM |
|---|---|---|---|---|---|---|---|
| Agent definition | Markdown files (dir tree) | YAML file (flat, one file) | Python SDK | Python code | YAML + Python | Python code | Markdown files |
| Named patterns | **Yes (12)** | No | No | No | No | No | No |
| FSM support | **Native (deterministic)** | DAG + LLM-driven traversal | switch/if-else | Conditional edges | Flows + @router | Router primitive | No |
| Reflexion | **Native** | Manual | Manual | Manual | Not native | Manual Loop | No |
| Debate | **Native** | Not native | Not native | Manual | Not native | broadcast Team | No |
| Constitutional | **Native** | Not native | Not native | Manual | Not native | Manual | No |
| Best-of-N | **Native** | Not native | Voting approx. | Manual | Not native | Manual Parallel | No |
| Chain-of-verification | **Native** | Not native | Not native | Manual | Not native | Manual | No |
| Runtime | Rust binary | Go binary | Microservices | Python | Python | Python | None (spec only) |
| Zero app code | **Yes** | **Yes** | No | No | Partial | No | Yes (no runtime) |
| Local-first | **Yes** | **Yes** | No (hosted) | Yes | Yes | Yes | N/A |
| Stars | — | 2.7k | 6.6k | 27.7k | 47.4k | 39k | 20 |

---

## 1. Docker Agent

**GitHub:** github.com/docker/docker-agent · 2,721 stars · Go · Built by Docker Engineering

### What it is

A local Go binary (also ships as a Docker Desktop CLI plugin) that reads a single YAML file defining one or more agents. Zero cloud dependency. Agents run locally. Backed by Docker Inc — actively developed, growing fast.

### Agent definition

Everything lives in one YAML file per project:

```yaml
version: "8"
agents:
  root:
    model: anthropic/claude-sonnet-4-5
    instruction: |
      You are a research assistant.
    sub_agents: [researcher, writer]
    toolsets:
      - type: filesystem
      - type: mcp
        ref: docker:duckduckgo
```

### Multi-agent composition — it's a DAG, not FSM

Docker Agent's composition model is fundamentally a **DAG with LLM-driven edge traversal**, not agent composition in the tama sense.

**`sub_agents:`** — hierarchical delegation. The parent agent gets a `transfer_task(agent, task)` tool injected. The parent LLM *decides* at runtime which child to call and what task to give it. The child is isolated (no parent history). This is tool-calling with agents as tools — not a declared topology.

**`handoffs:`** — each agent declares which other agents it *can* hand off to (an adjacency list). But which edge to traverse is decided by the LLM at runtime, not by a routing table. There are no named states, no routing words, no deterministic dispatch.

**Critical limitation — no pattern nesting:** In Docker Agent, all agents live in a single flat YAML file. There is no concept of "this sub-agent runs the reflexion pattern" or "this worker uses a critic loop." An agent is just a system prompt + tools. You cannot declare a topology *inside* a sub-agent. The composition is flat delegation, not recursive pattern composition.

**tama's agent composition is fundamentally different:**
- Each agent lives in its own directory (`agents/name/AGENT.md`) with its own declared `pattern:`
- A sub-agent in tama can itself be `pattern: reflexion` — it runs the full act→reflect→loop internally
- The full graph is parsed and validated *statically* before any LLM call (`AgentGraph::build()`)
- Routing is deterministic: the LLM returns a routing word (`key` in `finish(key, value)`), the FSM table maps that word to the next state — no LLM decides "which agent to call next"

**`background_agents` toolset** — true parallelism. `run_background_agent(agent, task)` is non-blocking; coordinator fans out and polls for results.

### Does it have FSM?

**No — it has a DAG with LLM-driven traversal.** The `handoffs:` config defines which edges *exist* (possible transitions), but which edge to take is decided by the LLM based on instructions — not by a routing table keyed on output words. There is no `state:`, no `on_enter:`, no routing words, no deterministic dispatch. Even the cycles that `handoffs:` supports are traversed by LLM decision, not by declared transition rules.

tama's `pattern: fsm` works differently: the LLM outputs a routing word (`key`), and the FSM *table* in the AGENT.md frontmatter maps that word to the next state deterministically. The LLM never "chooses an agent" — it produces output, and the runtime routes based on a declared rule.

### Patterns natively supported

None are named or declared. Everything is assembled from three primitives: delegation, handoffs, and background parallelism. Reflexion, debate, critic, constitutional AI, best-of-N, chain-of-verification — all require hand-coding via instruction engineering.

| Pattern | tama | Docker Agent |
|---------|------|-------------|
| ReAct | `pattern: react` | Default — every agent is a ReAct loop |
| Scatter/parallel | `pattern: scatter` | `background_agents` toolset |
| Hierarchical | Not implemented | `sub_agents:` |
| Reflexion | `pattern: reflexion` | Manual — two sub_agents calling each other in a loop |
| Debate | `pattern: debate` | Not native |
| FSM | `pattern: fsm` | Not native (LLM-driven handoff graph only) |
| Critic | `pattern: critic` | Not native |
| Constitutional | `pattern: constitutional` | Not native |
| Best-of-N | `pattern: best-of-n` | Not native |
| Chain-of-verification | `pattern: chain-of-verification` | Not native |

### Where Docker Agent is stronger

- **Rich tooling**: 13 built-in toolset types (shell, filesystem, lsp, think, todo, memory, tasks, fetch, api, openapi, a2a, rag, background_agents)
- **Full MCP ecosystem**: Docker MCP catalog + local stdio + remote SSE
- **OCI packaging**: `docker agent share push` — publish/pull agents from any registry
- **A2A protocol**: serve and consume agents across frameworks
- **Hooks**: pre/post tool, session start/end, stop, notification — full shell script control over every tool call
- **RAG built-in**: BM25 + embeddings + hybrid + reranking via `rag:` config
- **Model routing**: per-turn semantic routing between models based on examples
- **Thinking budgets**: `thinking_budget: high/low` per model config
- **Docker Model Runner**: local offline inference, no API key needed

### Key difference vs tama

Three fundamental differences:

1. **DAG vs FSM.** Docker Agent's routing is LLM-driven over a declared adjacency list (DAG). tama's FSM routing is deterministic: the LLM outputs a key, the runtime maps it to the next state via a table. No LLM in tama ever "chooses which agent to call next."

2. **Flat vs recursive composition.** All Docker Agent agents live in one flat YAML file. Sub-agents are system prompts with tools — they have no internal pattern structure. In tama, every agent in the graph declares its own `pattern:`, so a scatter worker can internally be a reflexion loop, which itself calls critic agents.

3. **LLM decides routing vs runtime decides routing.** In Docker Agent, if you want "go back to step A if quality is low," you rely on the LLM following instructions. In tama's FSM, the routing table is declared in YAML — the LLM returns `"retry"` or `"done"`, and the runtime deterministically routes. No prompt engineering required for the control flow.

---

## 2. Julep

**GitHub:** github.com/julep-ai/julep · 6,603 stars · Python · **Hosted service shut down Dec 31, 2025. Now self-host only. Team has moved on.**

### What it is

A microservices platform (Temporal + Postgres + 8 services) for durable, long-running agent tasks. Tasks are defined as YAML programs passed via Python SDK. Self-hosting requires a full Docker Compose stack.

### Task definition

YAML is passed as a dict via Python SDK — not a file on disk:

```python
client.tasks.create(agent_id=agent_id, **{
    "name": "Research Pipeline",
    "main": [
        {"prompt": [{"role": "user", "content": "$ _.topic"}]},
        {"tool": "brave_search", "arguments": {"query": "$ _.result"}},
        {"if": "$ len(_.results) > 0",
         "then": [{"return": "$ _.results"}],
         "else": [{"error": "No results"}]}
    ]
})
```

### Patterns natively supported

Julep documents these explicitly:

| Pattern | Implementation |
|---------|---------------|
| Prompt chaining | Sequential `main:` steps |
| Routing | `switch:` / `if-else:` on LLM output |
| Parallelization (sectioning) | `over`/`map` with `parallelism: N` |
| Voting (best-of-N approx.) | `over` + custom aggregation |
| Evaluator-optimizer (reflexion approx.) | Recursive `workflow:` calls with `if-else` scoring |
| Orchestrator-workers | `foreach:` calling sub-workflows |

No native support for: debate, constitutional AI, chain-of-verification, FSM, scatter as named concepts.

### Does it have FSM?

**No.** `switch:` and `if-else:` provide conditional branching, but there are no named states, no `on_enter`/`on_exit`, no state-file-per-state pattern. Steps are indexed, not named states.

### Why it matters less now

The hosted backend shut down. Self-hosting requires running ~10 Docker services. The team has moved to a different product (`memory.store`). For new projects, Julep is effectively abandoned despite the star count.

---

## 3. LangChain / LangGraph

**GitHub:** langchain-ai/langgraph · 27,760 stars · Python

### What it is

LangGraph is a graph execution engine for stateful, cyclic agent workflows. LangChain is the broader ecosystem of model wrappers and integrations. LangGraph is the orchestration layer.

### Agent definition — code only

No YAML, no Markdown, no config files. Everything is Python:

```python
from langgraph.graph import StateGraph, START, END
from typing_extensions import TypedDict, Annotated
from operator import add

class State(TypedDict):
    messages: Annotated[list, add]

def llm_call(state: State):
    return {"messages": [model.invoke(state["messages"])]}

def tool_node(state: State):
    # execute tool calls from last message
    ...

def should_continue(state: State):
    if state["messages"][-1].tool_calls:
        return "tools"
    return END

builder = StateGraph(State)
builder.add_node("llm", llm_call)
builder.add_node("tools", tool_node)
builder.add_edge(START, "llm")
builder.add_conditional_edges("llm", should_continue, ["tools", END])
builder.add_edge("tools", "llm")
agent = builder.compile()
```

That's a minimal ReAct loop: ~35 lines. With `create_react_agent(model, tools)` prebuilt: 1 line — but it's opaque.

### Patterns

LangGraph has **zero named patterns**. Every pattern is assembled by the developer:

- **Reflexion**: two nodes (actor, reflector) + conditional edge checking "DONE"
- **Critic/refine**: three sequential nodes chained with edges
- **Debate**: parallel nodes + judge node + merge edge
- **FSM**: conditional edges returning string node names — the graph IS a state machine
- **Scatter**: `Send` API returns a list of `Send("node", item)` objects from a conditional edge
- **Plan-execute**: planner node + loop with executor + verifier

Prebuilt helpers: `create_react_agent`, `create_supervisor` (langgraph-supervisor package), `ToolNode`. Everything else is DIY.

### State management

State is a `TypedDict` with `Annotated` reducers. Checkpointing (thread-scoped persistence, time-travel debugging, human-in-the-loop via `interrupt()`) is available with `InMemorySaver` or `PostgresSaver`. This is LangGraph's strongest feature — no other framework in this comparison has equivalent state durability.

### Developer experience

Boilerplate is significant. The mental model requires: graph theory, TypedDict typing, reducer functions, understanding of the node/edge/state separation. Frequent breaking changes in 2023–2024. LangSmith (paid) is almost required for production observability.

### Key difference vs tama

LangGraph routing requires Python. Every branch, every cycle, every conditional is a `def should_continue(state)` function. The developer writes the control flow in code — nodes and edges, not states and transitions.

tama's FSM inverts this: the developer declares a state table in YAML, and the LLM produces routing words that the runtime maps to transitions. No Python, no graph construction, no reducer functions. The topology is readable without executing anything.

The tradeoff is real: LangGraph can implement any topology — tama's FSM is bounded by what the state table can express. For anything outside the built-in patterns, tama requires modifying Rust code. LangGraph has no such ceiling.

---

## 4. CrewAI

**GitHub:** crewAIInc/crewAI · 47,420 stars · Python

### What it is

A framework for autonomous, role-based multi-agent collaboration. The core abstraction is "a crew of agents working through tasks." Two separate systems: **Crews** (LLM-orchestrated) and **Flows** (code-orchestrated).

### Agent + task definition

Recommended approach: YAML config + Python decorators:

```yaml
# config/agents.yaml
researcher:
  role: "Senior Research Analyst"
  goal: "Find accurate information about {topic}"
  backstory: "Experienced researcher with 10 years in the field"
  llm: openai/gpt-4o

# config/tasks.yaml
research_task:
  description: "Research the latest developments in {topic}"
  expected_output: "A comprehensive 3-paragraph summary"
  agent: researcher
  output_file: output/research.md
```

```python
@CrewBase
class ResearchCrew:
    @agent
    def researcher(self) -> Agent:
        return Agent(config=self.agents_config['researcher'], tools=[SerperDevTool()])

    @task
    def research_task(self) -> Task:
        return Task(config=self.tasks_config['research_task'])

    @crew
    def crew(self) -> Crew:
        return Crew(agents=self.agents, tasks=self.tasks, process=Process.sequential)
```

The YAML defines agent **identity** (role, goal, backstory, model) and task **descriptions**. The **orchestration** (process type, task ordering, wiring) still lives in Python.

### Process types

- `Process.sequential` — tasks run in order; each task's output available as `context` to later tasks
- `Process.hierarchical` — a manager LLM delegates tasks to worker agents; LLM-driven routing

No parallel process at the crew level. Async execution (`async_execution=True` on tasks) enables concurrency for independent tasks.

### Does it have FSM?

**Flows** provide event-driven routing:

```python
class ReviewFlow(Flow[ReviewState]):
    @start()
    def write_draft(self): ...

    @router(write_draft)
    def review(self):
        return "approved" if quality_check() else "rejected"

    @listen("approved")
    def publish(self): ...

    @listen("rejected")
    def revise(self): ...
```

This is FSM-like: `@router` returns string labels, `@listen` triggers on labels. But it is not a full FSM — cycles require manually re-triggering methods, there are no `on_enter`/`on_exit` events, no formal state object beyond a Pydantic model.

### Patterns natively supported

| Pattern | CrewAI |
|---------|--------|
| Sequential pipeline | `Process.sequential` |
| Hierarchical / supervisor | `Process.hierarchical` |
| Conditional routing | Flows `@router` |
| Human-in-the-loop | `human_input=True` on task |
| Guardrails / retry | `guardrail=fn` on task with `guardrail_max_retries` |
| Reflexion | Not native |
| Debate | Not native |
| Constitutional | Not native (sequential tasks can approximate) |
| Best-of-N | Not native |
| Scatter/parallel fan-out | Not native |

### Key difference vs tama

CrewAI's YAML defines agent *identity* — role, goal, backstory. tama's Markdown defines agent *behavior* — pattern, model, prompt. In CrewAI, the orchestration topology is Python code. In tama, the pattern name in the frontmatter IS the orchestration declaration.

The stock analysis example: ~250 lines of Python in CrewAI vs 5 Markdown files in tama.

---

## 5. Agno (formerly Phidata)

**GitHub:** agno-agi/agno · 39,000 stars · Python · Apache-2.0 · Very active

### What it is

A full-stack Python framework covering three layers: agent/team framework, FastAPI production runtime (AgentOS), and cloud control plane (os.agno.com). Positioned as a production platform, not a prototyping tool.

### Agent definition — Python only

No YAML, no Markdown. Pure Python with ~70-parameter constructor:

```python
agent = Agent(
    name="Researcher",
    model=Claude(id="claude-sonnet-4-6"),
    instructions=["Research the topic thoroughly."],
    tools=[WebSearchTools(), WikipediaTools()],
    reasoning=True,
    reasoning_max_steps=10,
    memory_manager=AgentMemory(),
    output_schema=ResearchReport,  # Pydantic model
    pre_hooks=[PIIDetectionGuardrail()],
    retries=3,
    db=SqliteDb(db_file="agent.db"),
)
```

### Multi-agent teams

Three team modes:

**`TeamMode.route`** — leader selects ONE member, delegates entirely, returns that member's response.

**`TeamMode.broadcast`** — leader sends SAME task to ALL members in parallel, synthesizes all responses. (This is tama's `debate` and `parallel` in one.)

**`TeamMode.coordinate`** (default) — leader analyzes request, crafts individual subtasks per member, synthesizes a unified answer.

Teams are fully nestable: teams as members of teams.

### Workflows — code-orchestrated

The deterministic execution layer. Typed primitives:

```python
workflow = Workflow(
    name="Research Workflow",
    steps=[
        Router(selector=classify_fn, choices=[web_step, db_step]),
        Loop(steps=[refine_step], end_condition=quality_ok, max_iterations=3),
        Condition(evaluator=needs_check, steps=[fact_check_step]),
        Parallel([analysis_step, summary_step]),
        write_step,
    ],
)
```

| Primitive | Equivalent tama pattern |
|-----------|------------------------|
| `Loop` | `reflexion` (but code-defined end condition) |
| `Router` | `fsm` (but Python fn, not LLM routing words) |
| `Condition` | `fsm` conditional branch |
| `Parallel` | `parallel` |
| `Step` | any pattern step |

### Named patterns

None. Agno has the primitives to *build* every tama pattern, but none are pre-wired with names. `pattern: reflexion` in tama is a one-line declaration; the equivalent in Agno is a `Workflow` with a `Loop` wrapping two agents with an `end_condition` function.

### Where Agno is stronger than tama

- **Guardrails**: `pre_hooks`/`post_hooks` with PII detection, prompt injection blocking, custom validators
- **Memory**: agentic memory (agent decides what to remember), session summaries, user-scoped memory, RAG knowledge base
- **Structured output**: Pydantic/JSON Schema enforcement on any agent output
- **Production serving**: built-in FastAPI runtime with per-user session isolation, streaming, horizontal scaling
- **Human-in-the-loop**: `@approval` decorator on tools, `requires_confirmation=True` pause-and-resume
- **Tool ecosystem**: 100+ pre-built toolkits

### Key difference vs tama

Agno is a **full production platform** (serving, memory, guardrails, monitoring). tama is a **pattern-first local runtime**. Agno forces Python for everything. tama forces nothing except Markdown.

---

## 6. WSO2 Agent-Flavored Markdown (AFM)

**GitHub:** wso2/agent-flavored-markdown · 20 stars · Spec, no runtime · IUI 2026 research paper

### What it is

A portability specification for AI agents. Not a framework. Not a runtime. A file format standard: YAML frontmatter + Markdown body = one agent definition.

### File format

```yaml
---
spec_version: "0.3.0"
name: "Support Agent"
model:
  name: "claude-sonnet-4-6"
  provider: "anthropic"
  authentication:
    type: "api-key"
    api_key: "${env:ANTHROPIC_API_KEY}"
interfaces:
  - type: consolechat
tools:
  mcp:
    - name: "github"
      transport:
        type: http
        url: "https://mcp.github.com"
---

# Role
You are a customer support agent.

# Instructions
Help users resolve their issues clearly and concisely.
```

### Runtime and patterns

**No runtime in this repo.** WSO2's Agent Manager (closed-source, Choreo platform) is the reference implementation. No local execution story.

**No named patterns.** No multi-agent composition in v0.3.0 (listed as future work). No FSM, no reflexion, no debate. The only execution control knob is `max_iterations`.

### Why it matters for tama

WSO2 AFM independently arrived at nearly the same file format as tama's AGENT.md: YAML frontmatter + Markdown body = one agent. This validates the approach. Key divergences:

| | tama AGENT.md | WSO2 AFM |
|---|---|---|
| Pattern declaration | `pattern: reflexion` | Not in spec |
| Multi-agent | `agents/` directory tree | Not in spec (future) |
| Runtime | `tamar` Rust binary | None (closed WSO2) |
| Interfaces | CLI only | consolechat, webchat, webhook |
| Structured I/O | Not defined | JSON Schema `signature` |
| Skills | SKILL.md convention | agentskills.io spec |
| Stars | — | 20 |

The convergence path between tama's AGENT.md and AFM is short. Adding `spec_version`, `interfaces`, and `signature` to AGENT.md's frontmatter would make tama files AFM-compatible — worth considering as a positioning move.

---

## Competitive Position Summary

### What tama has that nobody else does

**1. A real FSM — the LLM produces content, the runtime owns control flow.**

This is tama's primary differentiator. Every other framework in this comparison routes via one of:

| Framework | Routing mechanism | Problem |
|-----------|------------------|---------|
| LangGraph | Python conditional edge functions | Requires code for every branch |
| Docker Agent | LLM decides which agent to hand off to | Non-deterministic — prompt-engineer your control flow |
| CrewAI Flows | Python `@router` decorator function | Requires code |
| Agno Workflow | Python `Router(selector=fn)` | Requires code |
| CrewAI Crews | Manager LLM delegates tasks | Non-deterministic |

tama does neither. The LLM returns a **routing word** (the `key` in `finish(key, value)`). The FSM state table in AGENT.md frontmatter maps that word to the next state deterministically. No code. No LLM making routing decisions. The developer declares the topology; the runtime enforces it.

```yaml
# agents/pipeline/AGENT.md
pattern: fsm
initial: triage
states:
  triage:
    - billing: billing-agent
    - technical: tech-agent
    - general: general-agent
  billing-agent:
    - done: ~
    - escalate: triage      # cycle back — declared, not hoped for
  tech-agent: ~
  general-agent: ~
```

The triage agent calls `finish(key="billing", value="...")`. The runtime routes to `billing-agent`. No Python function. No LLM choosing an agent. The triage agent never knows the routing table exists.

This makes complex routing — conditional branching, escalation paths, retry loops, cycles — a YAML authoring problem, not a programming problem.

**2. The Markdown body IS the system prompt.**

In LangGraph, CrewAI, and Agno, system prompts are Python string literals embedded in code. In tama, open `AGENT.md` and read the agent. One file, no translation layer between what you read and what runs.

**3. Step prompts are first-class versioned files.**

`act.md`, `reflect.md`, `draft.md` — each step's prompt is a separate file in git. You can PR-review a change to the reflexion prompt's reflection step independently of the act step. In every Python framework, step prompts are strings in a Python file — the diff is noise.

**4. Zero code for a complete multi-agent FSM system.**

Docker Agent comes closest but has no FSM — just LLM-driven DAG traversal. Every Python framework requires Python. tama requires only Markdown and YAML.

### Where tama has gaps

1. **No production serving.** CLI only. Agno and Julep have HTTP APIs. Docker Agent can expose agents via A2A.

2. **No memory or persistence.** Agno has a full memory system. LangGraph has checkpointers with time-travel. tama has DuckDB traces (observability only).

3. **No guardrails.** Agno has PII detection, prompt injection blocking, task-level retry validators. tama has none.

4. **No MCP ecosystem.** Docker Agent has the full Docker MCP catalog. LangGraph and CrewAI have LangChain tool integrations. tama has custom SKILL.md files per project.

5. **No orchestrator pattern.** The enum variant exists in the code but no `orchestrator.rs` is implemented.

6. **Small tool surface.** 100+ toolkits in Agno. tama requires authoring SKILL.md files.

### Biggest risks

**Docker Agent** is the most direct competitive threat. Backed by Docker Inc, actively growing, same "no Python required" philosophy. If they add a pattern library (reflexion, debate, constitutional, etc.), they eliminate tama's primary differentiator while bringing a far richer tooling ecosystem. Worth monitoring every release.

**Agno** is the most feature-complete framework in this list. If they add a YAML/Markdown agent definition layer — even partial — they close the gap significantly given their 39k stars and active community.

---

*This document covers: Docker Agent, Julep, LangGraph/LangChain, CrewAI, Agno, WSO2 AFM.*
