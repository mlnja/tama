---
name: search-web
description: Search the web via Jina AI — clean markdown results with no HTML parsing, no API key required.
tools: [tama_http_get]
---

Two-step process using Jina AI. Both steps return clean markdown — no HTML to parse.

## Step 1 — search

  https://s.jina.ai/?q=<url-encoded-query>

Returns a clean list of results: titles, URLs, and snippets. Encode spaces as `+`.

Example:
  tama_http_get("https://s.jina.ai/?q=rust+programming+language+adoption+2024")

Pick the 1-2 most relevant URLs from the results.

## Step 2 — read

  https://r.jina.ai/<full-url>

Fetches a page as clean markdown — no ads, no nav, no boilerplate. Use this for the best URL(s) from step 1.

Example:
  tama_http_get("https://r.jina.ai/https://en.wikipedia.org/wiki/Rust_(programming_language)")

## Rules

- At most 1 search call per query. Do not repeat the same query.
- At most 2 reader calls per search. Stop when you have enough content.
- Extract only what is relevant to your research question.
