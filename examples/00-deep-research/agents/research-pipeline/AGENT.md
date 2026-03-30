---
name: research-pipeline
description: Deep research pipeline with quality review. Researches a topic in parallel across multiple angles, then reviews quality and retries if needed.
version: 1.0.0
pattern: fsm
initial: research
states:
  research: reviewer
  reviewer:
    - approved: ~
    - retry: research
---
