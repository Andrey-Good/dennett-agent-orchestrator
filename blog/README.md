# Dennett Engineering Chronicle

This directory contains the first-person engineering chronicle of Dennett. It is deliberately outside `docs/`: the blog explains how decisions changed, while canonical specifications, architecture, Work Packages, tests and runtime evidence define what the product is.

## Layout

- `posts/` — finished or actively reviewed articles in publication order;
- `assets/NNN/` — public-safe visuals and their metadata for post `NNN`;
- `evidence/` — frozen, source-linked Evidence Packets used to write milestone posts;
- `notes/` — one small capture file for the current milestone, plus a reusable template;
- `INDEX.md` — publication order, status and continuity.

## Working rhythm

1. Open one milestone capture from `notes/_milestone_capture_template.md` when a stage has a plausible story.
2. During implementation, preserve only consequential facts: a failed hypothesis, a useful owner correction, a short log, a measured result, a screenshot candidate or a decision link.
3. At milestone closure, verify the chronology against commits, Completion Packets and tests; consolidate it into `evidence/Mxx.yaml`.
4. Delete or reset uncited scratch notes. The notes directory is a capture buffer, not a second project history.
5. Write the post, run privacy and link checks, request one independent editorial review, update `INDEX.md`, and only then mark it `published`.

`blog/AGENTS.md` owns the detailed editorial rules. Nothing in the blog can introduce a product requirement by itself.
