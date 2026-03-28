---
title: Installation
description: How to install tama and tamad on your machine.
---

## Prerequisites

- **Docker** (for `tama brew`)
- An API key for at least one supported LLM provider

## Homebrew (macOS / Linux)

```bash
brew install mlnja/tap/tama
```

## Build from source

Requires **Rust** 1.76+.

```bash
git clone https://github.com/mlnja/tama
cd tama
cargo build --release
```

This produces two binaries:
- `target/release/tama` — developer tool
- `target/release/tamad` — runtime

Add them to your PATH:

```bash
export PATH="$PATH:$(pwd)/target/release"
```

## Verify

```bash
tama --help
tama run --help
```

## Environment variables

tama uses environment variables to configure LLM providers and model roles.

### API keys

| Variable | Provider |
|----------|---------|
| `ANTHROPIC_API_KEY` | Anthropic (Claude) |
| `OPENAI_API_KEY` | OpenAI (GPT-4, etc.) |
| `GEMINI_API_KEY` | Google (Gemini) |

### Model roles

tama uses a **role-based model system**. Instead of hardcoding a model name in each agent, you assign roles (like `thinker`, `writer`, `fast`) and map them to models at runtime:

```bash
export TAMA_MODEL_THINKER="anthropic:claude-opus-4-6"
export TAMA_MODEL_WRITER="anthropic:claude-sonnet-4-6"
export TAMA_MODEL_FAST="anthropic:claude-haiku-4-5"
```

Agents reference roles:

```yaml
call:
  model:
    role: thinker
```

This lets you swap models without editing any agent files.

:::tip
You can also set models directly in `tama.toml` — see the [tama.toml reference](/reference/tama-toml).
:::

## Supported providers

| Provider | Format | Example |
|----------|--------|---------|
| Anthropic | `anthropic:model-id` | `anthropic:claude-sonnet-4-6` |
| OpenAI | `openai:model-id` | `openai:gpt-4o` |
| Google | `google:model-id` | `google:gemini-2.0-flash` |
