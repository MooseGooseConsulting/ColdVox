---
doc_type: prompt
subsystem: ci
status: draft
last_reviewed: 2026-05-13
freshness: historical
preservation: preserve
summary: "Prompt for reviewing and implementing an agentic evidence assessor."
signals: ['prompt', 'ci', 'agent-review', 'evidence']
---
# Meta-Prompt: Review, Critique, and Implement the Agentic Evidence Assessor

**Instructions for the User:** 
Copy and paste everything below the line into your next session with a capable AI agent (like Gemini CLI, Claude, or a custom coding agent) to have it review our work and finalize the implementation.

---

**Your Role:** You are a Senior DevOps Engineer and AI Architect. Your objective is to review a proposed system design for a "Shadow Mode Agentic Evidence Assessor," critique its technical feasibility, and then implement the final, hardened version in this repository.

## Context

We are attempting to implement a "Portable Agentic Evidence Standard" in this repository (`ColdVox`). The core philosophy is to move away from rigid, tautological unit tests and instead use an autonomous agent in CI to verify that Pull Requests provide empirical evidence for their material claims, and to detect Semantic Drift (code contradicting documentation).

We have drafted a plan to run `gemini-cli` natively in GitHub Actions as a "shadow" reviewer. It will read PRs and output a report to the GitHub Step Summary without blocking the build or commenting on the PR directly.

## Phase 1: Review & Critique

Before writing any code, you must read and critically evaluate the following files:
1.  `docs/reviews/portable_standard_critique.md` (The philosophy)
2.  `docs/reviews/reviewer_driven_evidence.md` (The workflow strategy)
3.  `docs/plans/agentic-evidence-preview.md` (The system architecture/spec)
4.  `.github/prompts/evidence-assessor.md` (The actual prompt the CI agent will execute)

**Perform a rigorous critique answering the following:**
*   **Prompt Robustness:** Look at `.github/prompts/evidence-assessor.md`. Is the Chain-of-Thought (CoT) rigorous enough for a complex LLM? Are there edge cases where the agent might hallucinate or get stuck in a loop trying to find "relevant documentation"?
*   **CI Execution Reality:** Look at the proposed GitHub Actions YAML in `agentic-evidence-preview.md`. Are we missing necessary GitHub token permissions? Does `actions/checkout@v4` with `fetch-depth: 0` actually provide enough context for the agent to run `git diff origin/main...HEAD` cleanly in a PR context? 
*   **Tooling Friction:** The CLI agent will need to use `read_file` and `grep_search`. Is the prompt explicit enough about *how* to find documentation, or will the agent fail because it doesn't know the exact file tree?

Provide your critique as a brief, bulleted report.

## Phase 2: Refine and Implement

Based on your critique, execute the following implementation:

1.  **Refine the Prompt:** Update `.github/prompts/evidence-assessor.md` to patch any logical holes or instructions you identified during your critique. Ensure it is perfectly tuned for a high-reasoning model (like Gemini 2.5 Pro or 3.1 Pro).
2.  **Write the GitHub Action:** Create `.github/workflows/agentic-evidence-preview.yml`. Ensure:
    *   It triggers correctly on PRs (`opened`, `synchronize`, `ready_for_review`).
    *   It securely injects the API key and GitHub token.
    *   It properly sets up the Git environment so `git diff` works flawlessly.
    *   It executes the agent in headless/autonomous mode (`-y` or `--yolo`).
3.  **Final Polish:** Verify that the system will successfully capture the agent's stdout/stderr or internal file-write and pipe it to `$GITHUB_STEP_SUMMARY`.

Execute your tool calls to create and modify these files now. Provide a brief summary of what you implemented.