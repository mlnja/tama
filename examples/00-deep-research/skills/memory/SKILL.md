---
name: memory
description: Store and retrieve values across agent steps in the same run. Use for preserving state that must survive FSM transitions.
tools: [tama_mem_set, tama_mem_get, tama_mem_append]
---

Key-value store scoped to the current run.

## Store a value

  tama_mem_set(key="task", value="the original research topic")

## Retrieve a value

  tama_mem_get(key="task")

Returns the stored string, or an empty string if not set.

## Append to a list

  tama_mem_append(key="notes", item="new item")

Appends to a newline-separated list. Useful for accumulating feedback across retries.
