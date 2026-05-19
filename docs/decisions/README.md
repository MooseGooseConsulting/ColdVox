# Architecture Decision Records

This directory holds Architecture Decision Records (ADRs) for ColdVox.

An ADR captures one architecturally-significant decision: what we decided,
what alternatives we considered, and what consequences follow. ADRs make
the reasoning behind design choices durable and discoverable so that
future contributors — human or agent — understand why the code looks the
way it does, and what guardrails matter.

We follow [Michael Nygard's format](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions)
with a few additions (explicit drivers and alternatives).

## When to write an ADR

Write an ADR when the decision:

- has an architectural blast radius (touches multiple crates, changes a
  cross-cutting contract, or constrains future design);
- has reasonable alternatives that someone would later ask about;
- involves a tradeoff that isn't obvious from reading the code; **or**
- is named by an AGENTS.md Working Rule as requiring one (e.g.
  *Canonical STT default changes require an ADR*).

Do not write an ADR for routine refactors, dependency bumps, naming
choices, or bug fixes that don't change behavior contracts.

## Format

Each ADR is a Markdown file named `ADR-NNN-kebab-title.md` with this skeleton:

```markdown
# ADR-NNN: Title (matching the filename)

- **Status:** PROPOSED | ACCEPTED | SUPERSEDED by ADR-XXX | DEPRECATED
- **Date:** YYYY-MM-DD

## Context
What problem are we solving? What constraints apply? Why now?

## Decision
What did we decide? State it as "We will <do X>." Be specific.

## Drivers
Bulleted considerations that pushed us toward the chosen option.

## Alternatives Considered
- **Option A:** description. Pros / Cons. Why not chosen.
- **Option B:** ...

## Consequences
Positive / Negative / Risks / Follow-ups.

## References
- Code: `path/to/file.rs`
- Commits: `<sha>` <title>
- Related ADRs: ADR-N, ADR-M
```

## Status workflow

- **PROPOSED** — under discussion; do not assume it reflects current code.
- **ACCEPTED** — the decision is in effect; code matches.
- **SUPERSEDED by ADR-XXX** — the decision was replaced; see the linked ADR.
- **DEPRECATED** — the decision no longer applies but has no replacement.

ADRs are append-only. **Never edit an ACCEPTED ADR's Decision section to
"fix" it after the fact** — write a new ADR that supersedes it. Editorial
fixes (typos, broken links, clarifying examples) are fine.

## Numbering

- Three-digit zero-padded number: `ADR-001-`, `ADR-002-`, …
- Number is permanent. Do not renumber when an ADR is superseded; mark
  the superseded ADR's status and link forward.
- One decision per ADR. Coupled decisions should be separate ADRs that
  reference each other.
