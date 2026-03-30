---
name: files
description: Write output files to disk. Use to persist final results as readable artifacts.
tools: [tama_files_write]
---

Write content to a file on disk.

  tama_files_write(path="report.md", content="...")

The file is written relative to the project root. Use markdown files for reports and text output.
