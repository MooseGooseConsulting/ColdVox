import sys
import os
import argparse

PATTERNS = [
    "delve into", "navigate the complexities", "in today's fast-paced world",
    "it's important to note that", "due to the fact that", "has the ability to",
    "leverage", "synergistic", "paradigm shift", "in order to"
]

def analyze_file(filepath, verbose=False):
    if not os.path.exists(filepath):
        print(f"File not found: {filepath}")
        return

    score = 0
    findings = []

    with open(filepath, 'r', encoding='utf-8') as f:
        lines = f.readlines()
        for i, line in enumerate(lines):
            line_lower = line.lower()
            for pattern in PATTERNS:
                if pattern in line_lower:
                    score += 10
                    findings.append((i + 1, pattern, line.strip()))

    print(f"Analysis for {filepath}")
    print(f"Overall slop score: {min(100, score)}")

    if verbose and findings:
        print("\nFindings:")
        for line_num, pattern, context in findings:
            print(f"Line {line_num} [{pattern}]: {context}")

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("file", help="File to analyze")
    parser.add_argument("--verbose", action="store_true", help="Print findings")
    args = parser.parse_args()

    analyze_file(args.file, args.verbose)
