#!/usr/bin/env python3
"""Mechanical Lint Gates for ColdVox

Deterministic CI checks that catch documentation lies, dead feature flags,
and config inconsistencies. No LLM inference - pure mechanical validation.

Usage:
    python scripts/lint_repo_integrity.py
    python scripts/lint_repo_integrity.py --check 1    # Run only check 1
    python scripts/lint_repo_integrity.py --strict-freshness  # Make check 5 blocking
    python scripts/lint_repo_integrity.py --fix-baseline # Update test skip baseline
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass, field
from datetime import datetime, timedelta
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).parent.parent
BASELINE_FILE = REPO_ROOT / ".test-skip-baseline.json"
DOCS_DIR = REPO_ROOT / "docs"


@dataclass
class CheckResult:
    name: str
    passed: bool
    message: str
    details: list[str] = field(default_factory=list)
    is_warning: bool = False


@dataclass
class Frontmatter:
    doc_type: str | None = None
    subsystem: str | None = None
    status: str | None = None
    last_reviewed: str | None = None
    freshness: str | None = None
    preservation: str | None = None


def print_result(result: CheckResult) -> None:
    """Print a check result in the standardized format."""
    if result.is_warning:
        status = "WARN"
    elif result.passed:
        status = "PASS"
    else:
        status = "FAIL"
    
    print(f"[{status}] {result.name}", end="")
    if result.message:
        print(f" ({result.message})")
    else:
        print()
    for detail in result.details:
        print(f"       - {detail}")


def run_command(cmd: list[str], cwd: Path | None = None, timeout: int = 300) -> tuple[int, str, str]:
    """Run a shell command and return exit code, stdout, stderr."""
    try:
        result = subprocess.run(
            cmd,
            cwd=cwd or REPO_ROOT,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return result.returncode, result.stdout, result.stderr
    except subprocess.TimeoutExpired:
        return 1, "", f"Command timed out after {timeout}s"
    except Exception as e:
        return 1, "", str(e)


def parse_toml(content: str) -> dict[str, Any]:
    """Parse TOML content using tomllib (Python 3.11+)."""
    import tomllib
    return tomllib.loads(content)


def parse_yaml_frontmatter(content: str) -> tuple[Frontmatter | None, str]:
    """Parse YAML frontmatter from markdown content.
    Returns (frontmatter, remaining_content).
    """
    if not content.startswith("---"):
        return None, content

    end_match = re.search(r"\n---\s*\n", content[3:])
    if not end_match:
        return None, content

    frontmatter_text = content[3:3 + end_match.start()]
    remaining = content[3 + end_match.end():]

    fm = Frontmatter()
    for line in frontmatter_text.split("\n"):
        line = line.strip()
        if not line or line.startswith("#"):
            continue

        match = re.match(r"^([a-zA-Z_][a-zA-Z0-9_]*):\s*(.+)?$", line)
        if match:
            key, value = match.groups()
            value = (value or "").strip().strip('"').strip("'")

            if key == "doc_type":
                fm.doc_type = value
            elif key == "subsystem":
                fm.subsystem = value
            elif key == "status":
                fm.status = value
            elif key == "last_reviewed":
                fm.last_reviewed = value
            elif key == "freshness":
                fm.freshness = value
            elif key == "preservation":
                fm.preservation = value

    return fm, remaining


def is_valid_feature_name(name: str) -> bool:
    """Check if a string is a valid Cargo feature name.
    Features can contain letters, numbers, hyphens, and underscores.
    Must start with a letter.
    """
    if not name:
        return False
    # Must start with letter
    if not name[0].isalpha():
        return False
    # Can contain letters, numbers, hyphens, underscores
    return all(c.isalnum() or c in '-_' for c in name)


def extract_features_from_docs() -> set[str]:
    """Extract feature names mentioned in AGENTS.md and README.md."""
    features: set[str] = set()

    doc_files = [REPO_ROOT / "AGENTS.md", REPO_ROOT / "README.md"]

    for doc_file in doc_files:
        if not doc_file.exists():
            continue

        content = doc_file.read_text()

        # Pattern 1: --features X (handles quoted and unquoted)
        for match in re.finditer(r'--features\s+(["\']?[a-zA-Z0-9_,\-]+["\']?)', content):
            feature_str = match.group(1).strip('"\'')
            for feat in feature_str.split(","):
                feat = feat.strip()
                if is_valid_feature_name(feat):
                    features.add(feat)

        # Pattern 2: features = ["X", "Y"] or features = [X, Y]
        for match in re.finditer(r'features\s*=\s*\[([^\]]+)\]', content):
            feature_str = match.group(1)
            for feat in re.finditer(r'["\']?([a-zA-Z0-9_\-]+)["\']?', feature_str):
                feat_name = feat.group(1)
                if is_valid_feature_name(feat_name):
                    features.add(feat_name)

        # Pattern 3: Feature list in AGENTS.md format: `- `feature` - description`
        for match in re.finditer(r'[-*]\s*`([a-zA-Z0-9_\-]+)`\s*-\s*(?:Faster-Whisper|NVIDIA|Platform|Silero|Example|Hardware)', content):
            feat = match.group(1)
            if is_valid_feature_name(feat):
                features.add(feat)

    return features


def get_app_cargo_features() -> dict[str, list[str]] | None:
    """Get features defined specifically in crates/app/Cargo.toml."""
    app_toml = REPO_ROOT / "crates" / "app" / "Cargo.toml"
    if not app_toml.exists():
        return None

    try:
        content = app_toml.read_text()
        data = parse_toml(content)
        if "features" in data:
            return data["features"]
    except Exception:
        pass
    return None


def check_feature_flag_sync() -> CheckResult:
    """Check 1: Verify documented features actually exist and compile."""
    doc_features = extract_features_from_docs()
    app_features = get_app_cargo_features()

    if app_features is None:
        return CheckResult(
            name="Feature-flag doc sync",
            passed=False,
            message="Could not read crates/app/Cargo.toml"
        )

    app_feature_names = set(app_features.keys())
    failures = []

    for feature in doc_features:
        if feature not in app_feature_names:
            failures.append(f"Feature '{feature}' documented but not in crates/app/Cargo.toml")
            continue

        exit_code, _, stderr = run_command(
            ["cargo", "check", "-p", "coldvox-app", "--features", feature, "--locked"],
            timeout=120
        )

        if exit_code != 0:
            failures.append(f"Feature '{feature}' failed cargo check")

    if failures:
        return CheckResult(
            name="Feature-flag doc sync",
            passed=False,
            message=f"{len(failures)} feature(s) have issues",
            details=failures
        )

    return CheckResult(
        name="Feature-flag doc sync",
        passed=True,
        message=f"{len(doc_features)} feature(s) verified"
    )


def find_features_used_in_code(crate_path: Path) -> set[str]:
    """Scan Rust source files in a crate for features used in cfg attributes."""
    used_features: set[str] = set()
    
    cfg_pattern = re.compile(r'cfg(?:\s*\(\s*feature\s*=\s*["\']([a-zA-Z0-9_\-]+)["\']|.*?feature\s*=\s*["\']([a-zA-Z0-9_\-]+)["\'])')
    
    for rs_file in crate_path.rglob("*.rs"):
        # Skip hidden directories and target
        if any(part.startswith('.') or part == 'target' for part in rs_file.relative_to(crate_path).parts):
            continue
        
        try:
            content = rs_file.read_text()
            for match in cfg_pattern.finditer(content):
                feat = match.group(1) or match.group(2)
                if feat:
                    used_features.add(feat)
        except Exception:
            continue
    
    return used_features


def check_dead_features() -> CheckResult:
    """Check 2: Find features defined as empty arrays that aren't aggregation features."""
    dead_features = []
    
    for cargo_toml in REPO_ROOT.rglob("Cargo.toml"):
        if "target" in str(cargo_toml):
            continue

        try:
            content = cargo_toml.read_text()
            data = parse_toml(content)

            if "features" not in data:
                continue

            features = data["features"]
            rel_path = cargo_toml.relative_to(REPO_ROOT)
            crate_dir = cargo_toml.parent

            # Find features used in code (via cfg) - crate-local only
            features_used_in_code = find_features_used_in_code(crate_dir)

            # Build set of all feature names that are referenced elsewhere
            referenced_features: set[str] = set()
            for feature_name, feature_deps in features.items():
                if isinstance(feature_deps, list):
                    for dep in feature_deps:
                        if isinstance(dep, str) and not dep.startswith("dep:"):
                            if "/" in dep:
                                referenced_features.add(dep.split("/")[1])
                            else:
                                referenced_features.add(dep)

            # Check for empty features that aren't referenced
            for feature_name, feature_deps in features.items():
                if isinstance(feature_deps, list) and len(feature_deps) == 0:
                    # Skip 'default' - it's a special feature that can be empty
                    if feature_name == "default":
                        continue
                    # Skip if it's referenced by other features
                    if feature_name in referenced_features:
                        continue
                    # Skip if it's used in code via cfg(feature = "...") in this crate
                    if feature_name in features_used_in_code:
                        continue
                    dead_features.append(f"{rel_path}: {feature_name} = []")

        except Exception:
            continue

    if dead_features:
        return CheckResult(
            name="Dead feature detection",
            passed=False,
            message=f"{len(dead_features)} dead feature(s) found",
            details=dead_features
        )

    return CheckResult(
        name="Dead feature detection",
        passed=True,
        message="No dead features found"
    )


def check_python_version_consistency() -> CheckResult:
    """Check 3: Verify Python version consistency across config files."""
    versions: dict[str, str] = {}

    py_version_file = REPO_ROOT / ".python-version"
    if py_version_file.exists():
        content = py_version_file.read_text().strip()
        if content:
            versions[".python-version"] = content

    mise_file = REPO_ROOT / "mise.toml"
    if mise_file.exists():
        try:
            content = mise_file.read_text()
            data = parse_toml(content)
            if "tools" in data and "python" in data["tools"]:
                versions["mise.toml"] = str(data["tools"]["python"])
        except Exception:
            pass

    pyproject_file = REPO_ROOT / "pyproject.toml"
    if pyproject_file.exists():
        try:
            content = pyproject_file.read_text()
            data = parse_toml(content)
            if "project" in data and "requires-python" in data["project"]:
                versions["pyproject.toml"] = data["project"]["requires-python"]
        except Exception:
            pass

    for dockerfile in REPO_ROOT.rglob("Dockerfile*"):
        if "target" in str(dockerfile):
            continue

        try:
            content = dockerfile.read_text()
            match = re.search(r'FROM\s+python:(\d+(?:\.\d+)?)', content, re.IGNORECASE)
            if match:
                rel_path = dockerfile.relative_to(REPO_ROOT)
                versions[str(rel_path)] = match.group(1)
        except Exception:
            pass

    if not versions:
        return CheckResult(
            name="Config consistency (Python version)",
            passed=True,
            message="No Python version declarations found"
        )

    def parse_major_minor(value: str) -> tuple[int, int] | None:
        match = re.search(r'(\d+)\.(\d+)', value)
        if not match:
            return None
        return (int(match.group(1)), int(match.group(2)))

    def requirement_allows_exact(requirement: str, exact: tuple[int, int]) -> bool | None:
        saw_constraint = False
        for raw_part in requirement.split(","):
            part = raw_part.strip()
            match = re.match(r'(>=|>|<=|<|==)?\s*(\d+\.\d+)', part)
            if not match:
                continue

            saw_constraint = True
            op = match.group(1) or "=="
            version = parse_major_minor(match.group(2))
            if version is None:
                continue

            if op == ">=" and exact < version:
                return False
            if op == ">" and exact <= version:
                return False
            if op == "<=" and exact > version:
                return False
            if op == "<" and exact >= version:
                return False
            if op == "==" and exact != version:
                return False

        return True if saw_constraint else None

    exact_versions: dict[str, tuple[int, int]] = {}
    range_versions: dict[str, str] = {}
    for source, version in versions.items():
        if any(op in version for op in [">", "<", "=", "~"]):
            range_versions[source] = version
            continue

        parsed = parse_major_minor(version)
        if parsed:
            exact_versions[source] = parsed

    unique_exact_versions = set(exact_versions.values())
    if len(unique_exact_versions) == 1:
        exact = next(iter(unique_exact_versions))
        incompatible = [
            f"{source}: {requirement}"
            for source, requirement in sorted(range_versions.items())
            if requirement_allows_exact(requirement, exact) is False
        ]
        if not incompatible:
            return CheckResult(
                name="Config consistency (Python version)",
                passed=True,
                message=f"Python {exact[0]}.{exact[1]} satisfies all declared ranges"
            )

    normalized: dict[str, str] = {}
    for source, version in versions.items():
        match = re.search(r'(\d+\.\d+)', version)
        if match:
            normalized[source] = match.group(1)
        else:
            normalized[source] = version

    unique_versions = set(normalized.values())

    if len(unique_versions) == 1:
        version = list(unique_versions)[0]
        return CheckResult(
            name="Config consistency (Python version)",
            passed=True,
            message=f"Python {version} everywhere"
        )

    details = [f"{source}: {version}" for source, version in sorted(versions.items())]
    return CheckResult(
        name="Config consistency (Python version)",
        passed=False,
        message=f"Inconsistent Python versions: {', '.join(sorted(unique_versions))}",
        details=details
    )


def check_frontmatter_completeness() -> CheckResult:
    """Check 4: Verify all docs have required frontmatter fields."""
    required_fields = ["doc_type", "subsystem", "status"]
    failures = []
    total_files = 0
    passing_files = 0

    for md_file in DOCS_DIR.rglob("*.md"):
        if md_file.name == "revision_log.csv":
            continue

        total_files += 1
        content = md_file.read_text()

        frontmatter, _ = parse_yaml_frontmatter(content)

        if frontmatter is None:
            rel_path = md_file.relative_to(REPO_ROOT)
            failures.append(f"{rel_path}: missing frontmatter")
            continue

        missing_fields = []
        if not frontmatter.doc_type:
            missing_fields.append("doc_type")
        if not frontmatter.subsystem:
            missing_fields.append("subsystem")
        if not frontmatter.status:
            missing_fields.append("status")

        if missing_fields:
            rel_path = md_file.relative_to(REPO_ROOT)
            failures.append(f"{rel_path}: missing {', '.join(missing_fields)}")
        else:
            passing_files += 1

    if failures:
        return CheckResult(
            name="Frontmatter completeness",
            passed=False,
            message=f"{len(failures)} file(s) missing fields ({passing_files}/{total_files} passing)",
            details=failures
        )

    return CheckResult(
        name="Frontmatter completeness",
        passed=True,
        message=f"All {total_files} file(s) have complete frontmatter"
    )


def check_stale_docs(strict: bool = False) -> CheckResult:
    """Check 5: Warn on stale documentation (>6 months old or marked dead)."""
    warnings = []
    six_months_ago = datetime.now() - timedelta(days=180)

    for md_file in DOCS_DIR.rglob("*.md"):
        if md_file.name == "revision_log.csv":
            continue

        content = md_file.read_text()
        frontmatter, _ = parse_yaml_frontmatter(content)

        if frontmatter is None:
            continue

        rel_path = md_file.relative_to(REPO_ROOT)

        if frontmatter.last_reviewed:
            try:
                date_str = frontmatter.last_reviewed
                for fmt in ["%Y-%m-%d", "%Y-%m", "%Y"]:
                    try:
                        reviewed_date = datetime.strptime(date_str, fmt)
                        if reviewed_date < six_months_ago:
                            warnings.append(f"{rel_path}: last_reviewed {date_str} (>6 months)")
                        break
                    except ValueError:
                        continue
            except Exception:
                pass

        is_archive = "docs/archive/" in str(rel_path)
        if not is_archive:
            if frontmatter.freshness == "dead":
                warnings.append(f"{rel_path}: freshness=dead, should be archived")
            if frontmatter.preservation == "delete":
                warnings.append(f"{rel_path}: preservation=delete, should be deleted")

    if warnings:
        return CheckResult(
            name="Stale doc detection",
            passed=not strict,
            is_warning=not strict,
            message=f"{len(warnings)} file(s) with warnings",
            details=warnings
        )

    return CheckResult(
        name="Stale doc detection",
        passed=True,
        message="No stale docs detected"
    )


def check_test_skip_audit(fix_baseline: bool = False) -> CheckResult:
    """Check 6: Count skipped tests and enforce ratchet."""
    # Run cargo test and capture output
    exit_code, stdout, stderr = run_command(
        ["cargo", "test", "--workspace", "--locked", "--", "--format", "json"],
        timeout=600
    )

    # If JSON format fails, try normal output
    if exit_code != 0:
        exit_code, stdout, stderr = run_command(
            ["cargo", "test", "--workspace", "--locked"],
            timeout=600
        )

    # If tests failed to run, that's a check failure
    if exit_code != 0:
        return CheckResult(
            name="Test skip audit",
            passed=False,
            message="cargo test failed to execute",
            details=["Cannot count skipped tests when tests fail to run"]
        )

    skipped_count = 0

    # Try to parse JSON output
    for line in (stdout + stderr).split("\n"):
        line = line.strip()
        if not line:
            continue

        if line.startswith("{"):
            try:
                event = json.loads(line)
                if event.get("type") == "test" and event.get("event") == "ignored":
                    skipped_count += 1
            except json.JSONDecodeError:
                pass

        # Fallback: parse text output - sum across all test binaries
        if "test result:" in line:
            match = re.search(r'(\d+)\s+ignored', line)
            if match:
                skipped_count += int(match.group(1))

    # Read baseline
    baseline_count = None
    if BASELINE_FILE.exists():
        try:
            baseline_data = json.loads(BASELINE_FILE.read_text())
            baseline_count = baseline_data.get("skip_count")
        except (json.JSONDecodeError, KeyError):
            pass

    # Create baseline if it doesn't exist and --fix-baseline is passed
    if fix_baseline:
        if baseline_count is None or skipped_count != baseline_count:
            BASELINE_FILE.write_text(json.dumps({"skip_count": skipped_count}, indent=2))
            return CheckResult(
                name="Test skip audit",
                passed=True,
                message=f"Created/updated baseline with {skipped_count} skipped tests"
            )
        return CheckResult(
            name="Test skip audit",
            passed=True,
            message=f"Baseline already at {skipped_count} skipped tests"
        )

    # If no baseline exists, this is the first run - create it
    if baseline_count is None:
        BASELINE_FILE.write_text(json.dumps({"skip_count": skipped_count}, indent=2))
        return CheckResult(
            name="Test skip audit",
            passed=True,
            message=f"First run: created baseline with {skipped_count} skipped tests"
        )

    # Compare against baseline
    if skipped_count > baseline_count:
        return CheckResult(
            name="Test skip audit",
            passed=False,
            message=f"{skipped_count} skipped, baseline: {baseline_count} (increased!)",
            details=["Run with --fix-baseline to update if this is intentional"]
        )

    return CheckResult(
        name="Test skip audit",
        passed=True,
        message=f"{skipped_count} skipped, baseline: {baseline_count}"
    )


def main() -> int:
    skip_checks = os.environ.get('SKIP_INTEGRITY_CHECKS', '')
    skip_set = set(int(x.strip()) for x in skip_checks.split(',') if x.strip())

    parser = argparse.ArgumentParser(
        description="Mechanical Lint Gates for ColdVox"
    )
    parser.add_argument(
        "--check",
        type=int,
        choices=range(1, 7),
        metavar="N",
        help="Run only a specific check (1-6)"
    )
    parser.add_argument(
        "--strict-freshness",
        action="store_true",
        help="Make Check 5 (stale doc detection) blocking"
    )
    parser.add_argument(
        "--fix-baseline",
        action="store_true",
        help="Update the test skip baseline file"
    )

    args = parser.parse_args()

    checks = [
        (1, check_feature_flag_sync),
        (2, check_dead_features),
        (3, check_python_version_consistency),
        (4, check_frontmatter_completeness),
        (5, lambda: check_stale_docs(strict=args.strict_freshness)),
        (6, lambda: check_test_skip_audit(fix_baseline=args.fix_baseline)),
    ]

    results = []
    failed = False

    for check_num, check_func in checks:
        if check_num in skip_set:
            print(f"[SKIP] Check {check_num} skipped via SKIP_INTEGRITY_CHECKS")
            continue
        if args.check and args.check != check_num:
            continue

        try:
            result = check_func()
            results.append(result)
            print_result(result)

            if not result.passed:
                failed = True
        except Exception as e:
            error_result = CheckResult(
                name=f"Check {check_num}",
                passed=False,
                message=f"Error running check: {e}"
            )
            results.append(error_result)
            print_result(error_result)
            failed = True

    passed_count = sum(1 for r in results if r.passed)
    total_count = len(results)

    if failed:
        failed_count = total_count - passed_count
        print(f"RESULT: FAIL ({failed_count} check(s) failed)")
        return 1
    else:
        print(f"RESULT: PASS ({passed_count}/{total_count} checks passed)")
        return 0


if __name__ == "__main__":
    sys.exit(main())
