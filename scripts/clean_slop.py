import sys
import os
import argparse
import re

REPLACEMENTS = {
    r"(?i)\bdelve into\b": "examine",
    r"(?i)\bnavigate the complexities of\b": "handle",
    r"(?i)\bnavigate the complexities\b": "handle",
    r"(?i)\bin today's fast-paced world,?\s*": "",
    r"(?i)\bit's important to note that\s*": "",
    r"(?i)\bdue to the fact that\b": "because",
    r"(?i)\bhas the ability to\b": "can",
    r"(?i)\bleverage\b": "use",
    r"(?i)\bsynergistic\b": "cooperative",
    r"(?i)\bparadigm shift\b": "major change",
    r"(?i)\bin order to\b": "to"
}

def clean_file(filepath, save=False, output=None, aggressive=False):
    if not os.path.exists(filepath):
        print(f"File not found: {filepath}")
        return

    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()

    original_content = content

    for pattern, replacement in REPLACEMENTS.items():
        content = re.sub(pattern, replacement, content)

    # basic aggressive mode just capitalizes first letter if we stripped beginning of sentence
    if aggressive:
        content = re.sub(r"([.!?]\s+)([a-z])", lambda m: m.group(1) + m.group(2).upper(), content)

    if content == original_content:
        print("No slop patterns found to clean.")
        return

    if save:
        out_path = output if output else filepath

        if out_path == filepath:
            backup_path = f"{filepath}.backup"
            with open(backup_path, 'w', encoding='utf-8') as f:
                f.write(original_content)
            print(f"Created backup at {backup_path}")

        with open(out_path, 'w', encoding='utf-8') as f:
            f.write(content)
        print(f"Cleaned content saved to {out_path}")
    else:
        print("--- Preview of changes ---")
        # simple diff-like output
        orig_lines = original_content.splitlines()
        new_lines = content.splitlines()
        for i, (orig, new) in enumerate(zip(orig_lines, new_lines)):
            if orig != new:
                print(f"- {orig}")
                print(f"+ {new}")

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("file", help="File to clean")
    parser.add_argument("--save", action="store_true", help="Save changes (creates backup if overwriting)")
    parser.add_argument("--output", help="Save to different file")
    parser.add_argument("--aggressive", action="store_true", help="Aggressive mode")
    args = parser.parse_args()

    clean_file(args.file, args.save, args.output, args.aggressive)
