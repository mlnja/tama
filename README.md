<p align="center">
  <img src="docs/src/assets/logo.svg" alt="tama logo" width="140" />
</p>

<h1 align="center">tama 玉</h1>

<p align="center">
  Free, open-source, code-free orchestration for multi-agent workflows.
</p>

<p align="center">
  <a href="https://github.com/mlnja/tama/stargazers">
    <img src="https://img.shields.io/github/stars/mlnja/tama?style=flat-square" alt="GitHub stars" />
  </a>
  <a href="https://github.com/mlnja/tama/blob/main/LICENSE">
    <img src="https://img.shields.io/github/license/mlnja/tama?style=flat-square" alt="MIT license" />
  </a>
  <a href="https://tama.mlops.ninja/getting-started/introduction/">
    <img src="https://img.shields.io/badge/docs-tama.mlops.ninja-0A66C2?style=flat-square" alt="Docs" />
  </a>
  <img src="https://img.shields.io/badge/runtime-Rust-000000?style=flat-square&logo=rust" alt="Rust runtime" />
  <img src="https://img.shields.io/badge/orchestration-code--free-2EA44F?style=flat-square" alt="Code-free orchestration" />
</p>

<p align="center">
  <a href="https://tama.mlops.ninja/getting-started/quickstart/">Quickstart</a>
  ·
  <a href="https://tama.mlops.ninja/getting-started/hello-world-deep-research/">Hello World</a>
  ·
  <a href="https://github.com/mlnja/tama/tree/main/examples">Examples</a>
</p>

If you want to quickly try an agent flow, why are you still setting up a Python project, wiring graph nodes, defining state types, and writing routing functions before you can even iterate on the prompts?

That is the problem `tama` is trying to solve.

With `tama`, agents and skills are just Markdown files:

- agents live in `AGENT.md`
- skills live in `SKILL.md`
- orchestration is declared in YAML frontmatter
- routing can be an explicit FSM instead of hidden in code

By "code-free," we mean **no graph/orchestration code for the workflow itself**. You define the system in files instead of assembling it in Python.

## Why tama

- **No scaffold code for agent flows.** Use `tama init` and `tama add`, then start writing prompts.
- **Prompts as files.** Agents and skills are human-readable, diffable, and easy to reorganize.
- **Deterministic routing.** Use `fsm` when control flow should belong to the runtime, not the model.
- **Built-in patterns.** `react`, `fsm`, `scatter`, `critic`, `reflexion`, `debate`, `plan-execute`, and more.
- **Tracing included.** Inspect which agents ran, which tools were called, and which skills were loaded.
- **Rust runtime.** `tamad` is a native binary, not a Python orchestrator.

## What it looks like

This is a real workflow:

```yaml
---
name: support
pattern: fsm
initial: triage
states:
  triage:
    - billing: billing-agent
    - technical: tech-agent
  billing-agent:
    - done: ~
    - escalate: triage
  tech-agent: ~
---
```

Instead of writing routing code, you declare the transitions.

## Quick start

```bash
tama init my-project
cd my-project
tama add fsm support
tama add react triage
tama add react billing-agent
tama add react tech-agent
```

Then edit the generated `AGENT.md` files and run:

```bash
tama run "Customer says they were double charged and want a refund"
```

Or start with the docs:

- Quickstart: https://tama.mlops.ninja/getting-started/quickstart/
- Hello World: Deep Research: https://tama.mlops.ninja/getting-started/hello-world-deep-research/

## A more complete example

The best current example is the deep research workflow in [`examples/00-deep-research`](examples/00-deep-research).

It combines:

- `fsm` for the outer review loop
- `scatter` for fan-out research
- `react` workers for focused web research
- `memory` for retry-aware state
- `files` for writing `report.md`

Read the step-by-step walkthrough here:

https://tama.mlops.ninja/getting-started/hello-world-deep-research/

## Current CLI

```bash
tama init <name>              # create a new project
tama add <pattern> <name>     # scaffold an agent
tama add skill <name>         # scaffold a skill
tama lint                     # validate the project
tama run "your task"          # execute the entrypoint agent
```

## Example project structure

```text
my-project/
├── tama.toml
├── agents/
│   └── my-project-agent/
│       └── AGENT.md
└── skills/
```

Larger projects usually look like:

```text
my-project/
├── tama.toml
├── agents/
│   ├── pipeline/
│   │   └── AGENT.md
│   ├── worker/
│   │   └── AGENT.md
│   └── reviewer/
│       └── AGENT.md
└── skills/
    ├── search-web/
    │   └── SKILL.md
    └── memory/
        └── SKILL.md
```

## Patterns

Built-in patterns currently include:

- `oneshot`
- `react`
- `scatter`
- `parallel`
- `fsm`
- `critic`
- `reflexion`
- `constitutional`
- `chain-of-verification`
- `plan-execute`
- `debate`
- `best-of-n`
- `human`

## Docs

- Introduction: https://tama.mlops.ninja/getting-started/introduction/
- Installation: https://tama.mlops.ninja/getting-started/installation/
- Quickstart: https://tama.mlops.ninja/getting-started/quickstart/
- Hello World: Deep Research: https://tama.mlops.ninja/getting-started/hello-world-deep-research/

## Status

`tama` is usable now, but still early.

The best way to help is to:

- try a real workflow
- report DX pain
- file issues when something is unclear or broken
- tell us which examples feel useful and which feel toy-like

## License

[MIT](LICENSE)
