
from __future__ import annotations
import argparse
import os
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
NAV = ROOT / "docs/navigation"
SOURCES = [ROOT / "docs/specifications", ROOT / "docs/architecture"]

def slug(text: str, seen: dict[str, int]) -> str:
    text = re.sub(r"<[^>]+>|[`*_~]", "", text.strip().lower())
    text = re.sub(r"[^\w\-\s\u0400-\u04FF]", "", text)
    text = re.sub(r"\s+", "-", text).strip("-") or "section"
    n = seen.get(text, 0); seen[text] = n + 1
    return text if n == 0 else f"{text}-{n}"

def render(path: Path) -> str:
    rel = os.path.relpath(path, NAV).replace(os.sep, "/")
    out = [f"# Navigation: {path.name}", "", f"Canonical file: [`{path.name}`]({rel})", ""]
    seen: dict[str, int] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        m = re.match(r"^(#{1,3})\s+(.+?)\s*$", line)
        if m:
            level = len(m.group(1)); title = m.group(2)
            out.append("  " * (level - 1) + f"- [{title}]({rel}#{slug(title, seen)})")
    return "\n".join(out) + "\n"

def main() -> None:
    parser = argparse.ArgumentParser(); parser.add_argument("--check", action="store_true"); args = parser.parse_args()
    failed = False
    for base in SOURCES:
        for path in sorted(base.glob("*.md")):
            if path.name == "README.md": continue
            target = NAV / f"{path.stem}.toc.md"; content = render(path)
            if args.check:
                if not target.exists() or target.read_text(encoding="utf-8") != content:
                    print(f"stale navigation: {target.relative_to(ROOT)}"); failed = True
            else:
                target.write_text(content, encoding="utf-8", newline="\n")
    raise SystemExit(1 if failed else 0)
if __name__ == "__main__": main()
