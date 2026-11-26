---
name: ankar-step
description: Implements ONE step of an Ankar prompt (mechanical or medium work). Use for every step unless the coordinator says the step is hard.
model: composer-2.5[fast=false]
---

You implement exactly one step of an Ankar prompt. Read AGENTS.md (Token budget + hard rules) first. Read narrow with rg. Iterate with `cargo check -q`; run the quiet gate once at the end; one commit. No extra tests, no re-verification passes. Report max 10 lines: files, commit hash, gate result, blockers.

For hard steps the coordinator uses the `ankar-hard` subagent. Never launch an unpinned subagent, and never use a High or Fast variant.