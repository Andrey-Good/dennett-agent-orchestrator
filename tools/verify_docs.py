
from __future__ import annotations
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs"
errors: list[str] = []

for path in DOCS.rglob("*.md"):
    text = path.read_text(encoding="utf-8")
    if text.count("```") % 2:
        errors.append(f"unclosed code fence: {path.relative_to(ROOT)}")
    for target in re.findall(r"\[[^\]]+\]\(([^)]+)\)", text):
        if target.startswith(("http://", "https://", "mailto:", "#")):
            continue
        target_path = target.split("#", 1)[0]
        if not target_path:
            continue
        resolved = (path.parent / target_path).resolve()
        if ROOT.resolve() not in [resolved, *resolved.parents]:
            errors.append(f"link escapes repository: {path.relative_to(ROOT)} -> {target}")
        elif not resolved.exists():
            errors.append(f"missing local link: {path.relative_to(ROOT)} -> {target}")

if errors:
    print("Documentation verification failed:")
    print("\n".join(f"- {e}" for e in errors))
    sys.exit(1)
print(f"Documentation verification passed ({len(list(DOCS.rglob('*.md')))} markdown files).")
