# Tama Architecture RFC: From Skills Runtime to MCP-Native System

## Status
Draft

## Motivation

Current tama design centers around Skills as the primary execution abstraction.

However, based on architectural analysis:

- Skills introduce unnecessary reasoning in deterministic workflows
- CLI-based execution introduces runtime complexity
- Production systems require typed, observable, deterministic execution

This RFC proposes evolving tama into:

> **Filesystem-native MCP authoring + workflow runtime + optional skill layer**

---

## Core Principles

### 1. Separation of Concerns

| Layer | Responsibility |
|------|----------------|
| Workflow | Orchestration |
| Agent | Local reasoning |
| Skill | Guidance / playbooks |
| MCP | Execution contracts |
| Runtime | Execution environment |

---

### 2. Skills Are Not Execution

> Skills teach. They do not execute.

Skills:
- Provide strategy
- Provide decision-making heuristics
- Reference capabilities

Skills must NOT:
- Execute tools
- Wrap CLI commands
- Act as runtime boundaries

---

### 3. MCP Is the Execution Layer

Everything executable should be expressed as MCP:

- Tools → functions
- Resources → data/context
- Prompts → reusable templates

Reference:
https://modelcontextprotocol.io/specification/

---

## New Filesystem Layout

```
tama/
  agents/
    triage/AGENT.md

  skills/
    incident-triage/SKILL.md

  mcp/
    tools/
      fetch_url/
        TOOL.yaml
        handler.wasm

    resources/
      runbooks.incident-response/
        RESOURCE.yaml

    prompts/
      summarize-incident/
        PROMPT.md

  workflows/
    incident.workflow.yaml
```

---

## MCP Tools

Example:

```yaml
name: fetch_url
description: Fetch content from URL

inputSchema:
  type: object
  required: [url]
  properties:
    url:
      type: string
      format: uri

outputSchema:
  type: object
  properties:
    status:
      type: integer
    body:
      type: string

runtime:
  type: wasm
  module: handler.wasm
```

---

## MCP Resources

```yaml
uri: runbooks://incident-response
description: Incident response guide

source:
  type: file
  path: runbook.md
```

---

## MCP Prompts

```markdown
---
name: summarize_incident
---

Summarize this incident:

{{ incident }}
```

---

## Skills

```markdown
Use tools:
- fetch_url
- slack.send_message

Do not notify Slack before severity is known.
```

Rules:
- Skills are optional
- Skills are not required for execution
- Skills can reference MCP tools

---

## Workflows

```yaml
steps:
  - call: fetch_url
  - agent: triage
  - call: slack.send_message
```

Key rule:

> Workflow should not require skill discovery for known steps.

---

## Execution Backends

### Supported:

- Remote MCP server
- Local process
- WASM

### WASM Advantages:

- portable
- sandboxed
- deterministic
- no OS dependency

---

## CLI Deprecation Strategy

Current:
- Skills wrap CLI
- Execution happens via shell

Problems:
- string-based interface
- no schema
- environment dependency

Future:
- CLI only as implementation detail behind MCP tool
- never exposed to LLM

---

## New Runtime Model

### Exploratory Mode

```
Agent → read skill → choose MCP tools → execute
```

### Production Mode

```
Workflow → direct MCP call
```

---

## Migration Plan

### Phase 1
- Introduce MCP folder structure
- Keep existing skills

### Phase 2
- Allow workflows to call MCP directly

### Phase 3
- Remove requirement: tools via skills

### Phase 4
- Add WASM runtime

### Phase 5
- Add compiler/export

---

## CLI vs MCP

| Feature | CLI Skills | MCP |
|--------|------------|-----|
| Typing | ❌ | ✅ |
| Observability | ❌ | ✅ |
| Determinism | ❌ | ✅ |
| Portability | ❌ | ✅ |

---

## Final Philosophy

> tama packages agent systems  
> MCP executes  
> Skills guide  
> Workflows orchestrate  
> WASM runs

---

## Final Insight

> Do not use Skills as execution layer.  
> Use MCP for execution.  
> Use Skills for reasoning.

---

## Outcome

tama becomes:

- MCP-native
- portable
- typed
- production-ready

Instead of:

- skill-driven runtime
