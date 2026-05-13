#!/usr/bin/env python3
"""Generate docs/index.md from markdown frontmatter metadata."""

from __future__ import annotations

import datetime as dt
from pathlib import Path
from typing import Dict, List, Tuple

DOCS_ROOT = Path("docs")
INDEX_PATH = DOCS_ROOT / "index.md"

CORE_KEYS = {"doc_type", "subsystem", "status"}
INFO_KEYS = {"freshness", "preservation"}
GRACE_PERIOD_END = dt.date(2026, 3, 11)

VALID_FRESHNESS = {"current", "aging", "stale", "historical", "dead"}
VALID_PRESERVATION = {"reference", "preserve", "summarize", "delete"}


def parse_frontmatter(path: Path) -> Tuple[Dict, str]:
    text = path.read_text(encoding="utf-8")
    if not text.startswith("---\n"):
        return {}, "missing frontmatter"
    try:
        closing = text.index("\n---", 4)
    except ValueError:
        return {}, "unterminated frontmatter"

    header = text[4:closing]
    values: Dict = {}
    for line in header.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#") or ":" not in stripped:
            continue
        key, raw = stripped.split(":", 1)
        values[key.strip()] = raw.strip().strip("'\"")
    return values, ""


def load_existing_index_meta() -> Dict:
    if not INDEX_PATH.exists():
        return {}
    meta, err = parse_frontmatter(INDEX_PATH)
    if err:
        return {}
    return meta


def classify_doc(metadata: Dict, err: str, path: Path) -> Tuple[str, str, str]:
    """Returns (freshness, preservation, status_note)."""
    if err:
        return "invalid", "unknown", err

    freshness = metadata.get("freshness", "")
    preservation = metadata.get("preservation", "")

    if freshness not in VALID_FRESHNESS:
        freshness = "unclassified"
    if preservation not in VALID_PRESERVATION:
        preservation = "unclassified"

    missing_core = CORE_KEYS - set(metadata.keys())
    if missing_core:
        return "invalid", "unknown", f"missing core: {', '.join(sorted(missing_core))}"

    missing_info = INFO_KEYS - set(metadata.keys())
    if missing_info and dt.date.today() >= GRACE_PERIOD_END:
        return freshness, preservation, f"missing info keys (grace period ended)"
    elif missing_info:
        return (
            freshness,
            preservation,
            f"missing info keys (warn until {GRACE_PERIOD_END})",
        )

    if freshness == "dead" and "archive" not in path.parts:
        return freshness, preservation, "dead doc outside archive"

    return freshness, preservation, ""


def collect_docs() -> List[Tuple[Path, Dict, str, str, str]]:
    """Returns list of (path, metadata, freshness, preservation, note)."""
    docs: List[Tuple[Path, Dict, str, str, str]] = []
    # Sort by POSIX string so ordering is deterministic across platforms.
    # Path comparison on Windows is case-insensitive, which produces a
    # different order than the Linux-based docs-ci regenerator.
    for path in sorted(DOCS_ROOT.rglob("*.md"), key=lambda p: p.as_posix()):
        if path == INDEX_PATH:
            continue
        metadata, err = parse_frontmatter(path)
        freshness, preservation, note = classify_doc(metadata, err, path)
        docs.append((path, metadata, freshness, preservation, note))
    return docs


def build_summary_table(rows: List[Tuple], title: str, filter_fn) -> str:
    """Build a table for docs matching filter_fn."""
    filtered = [r for r in rows if filter_fn(r)]
    if not filtered:
        return f"## {title}\n\nNone.\n"

    lines = [
        f"## {title}\n",
        "| File | Summary | Freshness |",
        "| --- | --- | --- |",
    ]
    for path, meta, fresh, pres, note in filtered:
        rel = path.as_posix()
        summary = meta.get("summary", meta.get("doc_type", "-"))[:60]
        if note:
            fresh_display = f"{fresh} ({note})"[:30]
        else:
            fresh_display = fresh
        lines.append(f"| `{rel}` | {summary} | {fresh_display} |")

    return "\n".join(lines) + "\n"


def build_action_table(rows: List[Tuple]) -> str:
    """Build table of docs needing action."""

    def needs_action(r):
        _, _, fresh, pres, note = r
        return (
            bool(note)
            or fresh == "invalid"
            or (fresh == "dead" and "archive" not in r[0].parts)
        )

    filtered = [r for r in rows if needs_action(r)]
    if not filtered:
        return "## Action Required\n\nNo docs require immediate action.\n"

    lines = [
        "## Action Required\n",
        "| File | Issue | Suggested Action |",
        "| --- | --- | --- |",
    ]
    for path, meta, fresh, pres, note in filtered:
        rel = path.as_posix()
        if "missing frontmatter" in note:
            action = "Add frontmatter"
        elif "dead doc outside archive" in note:
            action = "Move to docs/archive/ or delete"
        elif "missing core" in note:
            action = "Add missing core fields"
        elif "missing info keys" in note:
            action = "Add freshness and preservation"
        else:
            action = "Review and fix"
        lines.append(f"| `{rel}` | {note or fresh} | {action} |")

    return "\n".join(lines) + "\n"


def build_preserve_table(rows: List[Tuple]) -> str:
    """Build table of docs with valuable ideas to preserve."""

    def should_preserve(r):
        _, _, fresh, pres, _ = r
        return pres in ("preserve", "reference") and fresh in (
            "current",
            "aging",
            "stale",
        )

    filtered = [r for r in rows if should_preserve(r)]
    if not filtered:
        return "## Preserve These Ideas\n\nNo docs marked for preservation.\n"

    lines = [
        "## Preserve These Ideas\n",
        "| File | Summary | Preservation | Signals |",
        "| --- | --- | --- | --- |",
    ]
    for path, meta, fresh, pres, _ in filtered:
        rel = path.as_posix()
        summary = meta.get("summary", "")[:50]
        signals = meta.get("signals", "")[:40]
        if isinstance(signals, str):
            signals = signals.strip("[]").replace("'", "").replace('"', "")
        lines.append(f"| `{rel}` | {summary} | {pres} | {signals} |")

    return "\n".join(lines) + "\n"


def build_archive_table(rows: List[Tuple]) -> str:
    """Build table of docs safe to archive or delete."""

    def should_archive(r):
        _, _, fresh, pres, _ = r
        return pres == "delete" or fresh == "dead" or pres == "summarize"

    filtered = [r for r in rows if should_archive(r)]
    if not filtered:
        return "## Safe to Archive/Delete\n\nNo docs flagged for archival.\n"

    lines = [
        "## Safe to Archive/Delete\n",
        "| File | Reason | Preservation |",
        "| --- | --- | --- |",
    ]
    for path, meta, fresh, pres, _ in filtered:
        rel = path.as_posix()
        if pres == "delete":
            reason = "No salvage value"
        elif pres == "summarize":
            reason = "Fold into other docs"
        elif fresh == "dead":
            reason = "References removed code"
        else:
            reason = "Low value"
        lines.append(f"| `{rel}` | {reason} | {pres} |")

    return "\n".join(lines) + "\n"


def build_stats(rows: List[Tuple]) -> str:
    total = len(rows)
    archived_count = sum(1 for r in rows if "archive" in r[0].parts)
    active_count = total - archived_count

    by_freshness = {}
    by_preservation = {}
    for _, _, fresh, pres, note in rows:
        by_freshness[fresh] = by_freshness.get(fresh, 0) + 1
        by_preservation[pres] = by_preservation.get(pres, 0) + 1

    invalid_count = by_freshness.get("invalid", 0)

    lines = [
        "# Documentation Index\n",
        f"Generated by `scripts/build_docs_index.py`.\n",
        f"- **Total docs**: {total}",
        f"- **Active docs**: {active_count}",
        f"- **Archived docs**: {archived_count}",
        f"- **Invalid/needs attention**: {invalid_count}\n",
        "### By Freshness",
    ]
    for f in sorted(by_freshness.keys()):
        lines.append(f"- {f}: {by_freshness[f]}")

    lines.append("\n### By Preservation")
    for p in sorted(by_preservation.keys()):
        lines.append(f"- {p}: {by_preservation[p]}")

    return "\n".join(lines) + "\n"


def main() -> None:
    rows = collect_docs()
    existing = load_existing_index_meta()
    today = dt.date.today().isoformat()

    index_last_reviewed = existing.get("last_reviewed", today)

    sections = [
        build_stats(rows),
        "",
        "---\n",
        build_action_table(rows),
        "",
        build_preserve_table(rows),
        "",
        build_archive_table(rows),
    ]

    content = f"""---
doc_type: index
subsystem: general
version: 2.0.0
status: active
owners: Documentation Working Group
last_reviewed: {index_last_reviewed}
freshness: current
preservation: reference
summary: "Auto-generated documentation index with freshness and preservation status"
---

{chr(10).join(sections)}
"""

    INDEX_PATH.write_text(content, encoding="utf-8")
    print(f"wrote {INDEX_PATH}")

    invalid = sum(1 for r in rows if r[2] == "invalid")
    dead_outside = sum(
        1 for r in rows if r[2] == "dead" and "archive" not in r[0].parts
    )

    if invalid > 0:
        print(f"[WARN] {invalid} docs have invalid frontmatter")
    if dead_outside > 0:
        print(f"[ERROR] {dead_outside} dead docs found outside archive/")

    needs_info = sum(1 for r in rows if "missing info keys" in r[4])
    if needs_info > 0 and dt.date.today() < GRACE_PERIOD_END:
        print(
            f"[INFO] {needs_info} docs missing freshness/preservation (grace period until {GRACE_PERIOD_END})"
        )


if __name__ == "__main__":
    main()
