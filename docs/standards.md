---
doc_type: standard
subsystem: general
version: 1.0.0
status: draft
owners: Documentation Working Group
last_reviewed: 2026-03-29
---

# Documentation Standards

This standard documents repository-specific enforcement notes derived from the Master Documentation Playbook v1.0.0.

## Metadata Schema

All Markdown files under `/docs` MUST include the canonical frontmatter:

```yaml
doc_type: [architecture|standard|playbook|reference|research|plan|troubleshooting|index]
subsystem: [domain-name|general]
version: 1.0.0
status: draft
owners: Documentation Working Group
last_reviewed: YYYY-MM-DD
```

### Frontmatter Template

Copy-paste this template into new documentation files:

```yaml
---
doc_type: reference
subsystem: general
version: 1.0.0
status: draft
owners: Your Name or Team
last_reviewed: YYYY-MM-DD
---
```

### Field Descriptions

- **doc_type**: Type of document
  - `architecture` - System design, component interactions, ADRs
  - `standard` - Coding standards, conventions, policies
  - `playbook` - Step-by-step guides, procedures, operational runbooks
  - `reference` - API docs, crate documentation, technical references
  - `research` - Investigations, experiments, proof-of-concepts
  - `plan` - Proposals, roadmaps, future work
  - `troubleshooting` - Debugging guides, known issues, solutions
  - `index` - Directory or category landing pages

- **subsystem**: Which part of ColdVox this relates to
  - `general` - Cross-cutting or repository-wide concerns
  - `audio` - Audio capture, processing, VAD
  - `foundation` - Core utilities, telemetry, shared infrastructure
  - `gui` - User interface, TUI dashboard
  - `stt` - Speech-to-text engine and plugins
  - `text-injection` - Keyboard/clipboard automation
## Domain Documentation Cross-links

Canonical strategy and behavior for the text injection system are documented under:
- `docs/domains/text-injection/ti-overview.md` – injector approaches, strategy order, rationale
- `docs/domains/text-injection/ti-unified-clipboard.md` – implementation details of the clipboard-based injector
- `docs/domains/text-injection/ti-testing.md` – live testing requirements and procedures

Teams should update these documents when changing injection ordering, adding/removing backends, or altering confirmation/prewarm behavior.
  - `vad` - Voice activity detection

- **version**: Document version following semantic versioning
  - Start with `1.0.0` for new docs
  - Increment MAJOR for breaking reorganizations
  - Increment MINOR for significant additions
  - Increment PATCH for minor fixes/clarifications

- **status**: Current state of the document
  - `draft` - Work in progress, under active development
  - `active` - Current, approved, and in use
  - `deprecated` - Outdated, replaced by newer docs

- **owners**: Who maintains this document
  - Individual name: "Jane Developer"
  - Team: "Documentation Working Group"
  - Multiple: "Audio Team, Jane Developer"

- **last_reviewed**: Date of last review (YYYY-MM-DD format)
  - Update when you make changes or verify accuracy
  - Helps identify stale documentation

### Auto-fix Missing Frontmatter

Pre-commit hooks will warn about missing frontmatter. To auto-fix files:

```bash
# Fix specific files
python3 scripts/ensure_doc_frontmatter.py --fix docs/path/to/file.md

# Fix all docs in a directory
python3 scripts/ensure_doc_frontmatter.py --fix docs/plans/*.md
```

The auto-fixer will:
- Infer `doc_type` from path (e.g., `docs/plans/` → `plan`)
- Infer `subsystem` from path (e.g., `docs/domains/audio/` → `audio`)
- Set sensible defaults for `version`, `status`
- Use your git author name for `owners`
- Set `last_reviewed` to today's date

**Always review auto-generated frontmatter** and adjust as needed before committing.

### Domain Filename Convention Enforcement

All domain documents under `docs/domains/<domain>/` must include the domain short code in the filename (prefix), e.g. `ti-overview.md`.

Pre-commit/CI enforcement:
- Run `python3 scripts/validate_domain_docs_naming.py` to validate naming.
- Old filenames may remain temporarily only if their frontmatter contains a `redirect:` key pointing to the new code-prefixed file.
- Each domain's overview MUST declare `domain_code: <code>` in its frontmatter.

## Documentation Placement

All Markdown documentation MUST reside under `/docs/` to maintain discoverability and consistency.

**Pre-commit enforcement**: The `check-markdown-placement` hook will **block commits** containing Markdown files outside `/docs/`.

### Approved Exceptions

The following files are explicitly allowed outside `/docs/`:

**Root-level files:**
- `README.md` - Repository overview and quick-start
- `CHANGELOG.md` - User-facing release notes
- `AGENTS.md` - Canonical AI agent instructions and doc-routing entry point
- `.github/copilot-instructions.md` - Thin shim pointing readers to `AGENTS.md` (retained only for tools that look at this path directly; no longer kept in sync via a script)
- `PR-NNN-*.md` - Pull request assessment documents (temporary)

**GitHub templates:**
- `.github/pull_request_template.md` - PR template for GitHub

**Crate READMEs:**
- `crates/*/README.md` - Package READMEs required for crates.io publishing

Crate READMEs are allowed but should be kept minimal (overview, installation, quick example). Place detailed documentation in `docs/reference/crates/<crate-name>/` instead.

Any additional exception requires approval from the Documentation Working Group and must be recorded here.

## Changelog Rubric

The root `CHANGELOG.md` follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) format and documents user-visible changes only.

### When to Update CHANGELOG.md

**MUST update** for:
- New features or functionality (user-facing)
- Breaking changes (API, configuration, behavior)
- Deprecations (features being phased out)
- Removed features or APIs
- Bug fixes affecting user experience
- Security fixes
- Major performance improvements
- Major dependency updates (version bumps, security patches)
- Major documentation overhauls (like documentation restructures)

**SHOULD update** for:
- Minor bug fixes
- Minor performance improvements
- Quality-of-life improvements
- Notable dependency updates

**SKIP changelog** for:
- Internal refactoring (no user impact)
- Test additions/changes
- CI/CD configuration
- Development tooling
- Documentation typos or minor clarifications
- Code formatting or linting

### Version Numbering (Semantic Versioning)

- **MAJOR (x.0.0)**: Breaking changes, incompatible API changes
- **MINOR (0.x.0)**: New features, backwards-compatible additions
- **PATCH (0.0.x)**: Bug fixes, backwards-compatible fixes

### Changelog Entry Format

```markdown
## [Unreleased]

### Added
- New feature description (#PR-number)

### Changed
- Changed behavior description (#PR-number)

### Deprecated
- Deprecated feature (#PR-number)

### Removed
- Removed feature (#PR-number)

### Fixed
- Bug fix description (#PR-number)

### Security
- Security fix description (#PR-number)

### Documentation
- Major documentation changes only (#PR-number)

### Dependencies
- Dependency update with rationale (#PR-number)
```

### Changelog Review Process

1. **PR Author**: Add changelog entry in PR if change is user-visible per rubric
2. **Reviewer**: Verify changelog entry exists and is appropriate
3. **Release Manager**: Move `[Unreleased]` entries to versioned release section

### Documentation Updates

All substantive documentation updates must:

1. Update the relevant section in this file if they introduce new exceptions or schema adjustments.
2. Append an entry to `docs/revision_log.csv` via the automated watcher.

## Watcher Specification

The docs CI workflow invokes the revision logger to append entries to `docs/revision_log.csv` for any Markdown file updates. Contributors MUST NOT edit the CSV manually; rely on the automation to ensure consistent auditing.
