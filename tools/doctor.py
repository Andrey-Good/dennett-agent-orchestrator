
from __future__ import annotations
import shutil

TOOLS = ["python", "git", "node", "pnpm", "rustc", "cargo", "buf", "protoc", "just"]
for tool in TOOLS:
    print(f"{tool:10} {shutil.which(tool) or 'missing'}")
