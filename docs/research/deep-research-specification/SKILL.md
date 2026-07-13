---
name: deep-research-specification
description: Conduct rigorous source-grounded research, challenge the user's and the agent's initial ideas, simulate real scenarios, choose the simplest design that survives criticism, and produce complete business-logic or system-specification documents with explicit acceptance, rejection, cost, risk, and validation criteria. Use for complex product concepts, agent systems, memory, workflows, architecture-adjacent business logic, technical strategy, or any task where a superficial literature summary is insufficient.
---

# Deep Research Specification

## Purpose

Use this skill when the goal is not merely to summarize sources, but to **design or revise a serious system from evidence**.

The output should combine:

- the user's actual goals and constraints;
- existing project documents;
- current primary research;
- official product and protocol documentation;
- production case studies;
- issue trackers and expert discussions as practical evidence;
- explicit criticism of all candidate solutions;
- scenario simulation;
- cost, latency, complexity, safety, and maintainability analysis;
- a complete, internally consistent specification artifact.

The skill must not treat the user's ideas, prior assistant output, popular frameworks, scientific papers, blog posts, or current industry fashion as unquestionable truth.

The objective is the **best justified practical design**, not the most complicated design and not the longest document.

---

# 1. Non-negotiable principles

## 1.1. Treat every proposal as a hypothesis

When the user proposes a design, interpret it as:

> “This may be a strong solution. Test it against alternatives, real constraints, failure cases, and current evidence.”

Do the same with earlier assistant documents.

Never preserve a previous decision merely because it was written formally.

## 1.2. Start from the simplest viable baseline

Before adding a subsystem, agent, workflow engine, schema, reviewer, state machine, service, file, or abstraction, define the simplest baseline that could solve the problem.

Examples:

- one model call;
- one strong agent with tools;
- a prompt or instruction;
- a reusable skill;
- a Markdown document;
- a lightweight runtime record;
- an existing provider capability;
- a simple adapter around a mature tool.

A more complex mechanism is accepted only if it demonstrably adds necessary value.

## 1.3. Optimize cost-of-success, not theoretical maximum score

Evaluate:

- task success;
- correctness;
- quality;
- token/provider usage;
- latency;
- user effort;
- coordination overhead;
- implementation burden;
- debugging cost;
- failure recovery;
- safety and reversibility.

A two-percent quality improvement can be a regression if it multiplies latency, token use, complexity, or maintenance.

## 1.4. Separate mandatory invariants from optional intelligence

Hard rules belong in deterministic or governed mechanisms only when violating them can corrupt state, leak secrets, repeat an external effect, lose data, or break a system contract.

Open-ended interpretation, taste, planning, research, decomposition, and creative adaptation should remain model-driven unless a recurring failure proves otherwise.

## 1.5. Prefer evidence over architectural aesthetics

Do not choose a graph, schema, multi-agent team, workflow engine, event sourcing, microservices, or visual builder because it looks sophisticated.

Each mechanism must have:

- a concrete problem;
- a simpler baseline;
- a measurable benefit;
- failure modes;
- rejection criteria;
- an exit or rollback path.

## 1.6. Preserve user control without transferring micromanagement

Distinguish:

- freedom of reasoning;
- number of agents;
- review depth;
- permission to create external effects;
- safety floor;
- user interruption frequency;
- cost/latency budget.

Do not collapse them into one vague “autonomy” variable.

## 1.7. Be honest about evidence strength

Distinguish:

- peer-reviewed or archival research;
- preprints;
- official documentation;
- production reports;
- controlled benchmarks;
- self-reported case studies;
- GitHub issues;
- forum discussions;
- personal opinion.

A useful anecdote is not a universal result. A benchmark result is not automatically production proof.

---

# 2. Decide whether a plan-for-the-plan is needed

Use a plan-for-the-plan when any of the following is true:

- the requested system spans several domains;
- more than one existing document must be reconciled;
- the user explicitly requests a rigorous research process;
- there are several competing architectures;
- current sources may be incomplete or contradictory;
- the output will become a foundation for architecture or code;
- the work can easily become long but shallow.

The plan-for-the-plan should answer:

1. What must the final artifact enable?
2. Which decisions are irreversible or high-impact?
3. Which user assumptions must be tested?
4. Which previous assistant assumptions must be tested?
5. What source classes are required?
6. What scenarios expose weak designs?
7. What acceptance and cancellation gates prevent research theatre?
8. What can be omitted if it does not improve the final decision?

Do not show private chain-of-thought. The visible plan should state goals, stages, and criteria, not hidden reasoning.

---

# 3. Build the research charter

Before broad browsing, create a compact internal charter.

## 3.1. Core question

Rewrite the user's request as one decision-oriented question.

Bad:

> Research agent memory.

Good:

> Design a practically scalable personal-agent memory that preserves open-ended human context, supports exact actions and current state, remains portable across projects, and beats simpler note-based baselines at acceptable cost.

## 3.2. Required scenarios

List end-to-end scenarios that the system must support.

For each scenario identify:

- trigger;
- actor;
- inputs;
- mutable state;
- expected result;
- evidence of success;
- user-visible behavior;
- failure and recovery;
- cost sensitivity;
- safety sensitivity.

## 3.3. Hypothesis register

Create hypotheses from:

- user proposals;
- previous documents;
- common industry patterns;
- your own candidate improvements.

For each hypothesis write:

- why it might be right;
- why it might be wrong;
- evidence needed;
- acceptance criteria;
- rejection criteria.

## 3.4. Explicit non-goals

State what the document will not choose yet, such as:

- programming language;
- specific database;
- deployment topology;
- UI visual style;
- one provider forever.

This prevents premature architecture decisions.

---

# 4. Retrieve project context first

When the task depends on user files or prior documents:

1. locate the latest authoritative versions;
2. read the relevant full sections, not only snippets;
3. identify terminology and normative statements;
4. create a contradiction list;
5. distinguish user-edited files from obsolete assistant versions;
6. avoid silently merging rejected concepts back into the new artifact.

Build a document ownership map:

- which file defines memory;
- which defines agents;
- which defines permissions;
- which is only a concept overview;
- which version supersedes another.

If the current task changes an existing document, preserve unaffected sections and explicitly state what is superseded.

---

# 5. Source strategy

## 5.1. Use multiple evidence layers

Search at least these layers when relevant:

### Primary research

- papers;
- benchmarks;
- ablations;
- system descriptions;
- failure taxonomies.

### Official documentation

- provider docs;
- protocol specifications;
- framework lifecycle and constraints;
- security and permission semantics.

### Production case studies

- engineering blogs;
- postmortems;
- migration reports;
- scale experiments;
- real cost and failure data.

### Practical criticism

- issue trackers;
- technical forum discussions;
- maintainers' comments;
- reports of regressions or overhead.

Use practical criticism as evidence of failure modes, not as proof of universal law.

## 5.2. Search in rounds

### Round 1 — landscape

Find the main approaches and vocabulary.

### Round 2 — strongest proponents

Find the best evidence for each serious candidate.

### Round 3 — criticism and failure

Search specifically for:

- “failure”;
- “overhead”;
- “cost”;
- “latency”;
- “regression”;
- “does not outperform”;
- “context loss”;
- “coordination”;
- “production issue”;
- “security”;
- “scaling”.

### Round 4 — real implementation

Find systems that actually ran at scale or over long periods.

### Round 5 — edge cases

Search for:

- offline and restart;
- external side effects;
- conflicting writers;
- stale data;
- retries;
- deletion;
- import/export;
- adversarial inputs;
- user interruption;
- budget exhaustion.

### Round 6 — current verification

Check dates, versions, deprecations, and whether a claimed “latest” system is still current.

## 5.3. Prefer primary sources for technical claims

When discussing how a framework or API works, use official documentation or source code.

When discussing research results, use the paper itself.

When discussing production experience, use the original engineering report when available.

## 5.4. Create a source ledger

For each source record:

- title;
- date;
- source type;
- claim it supports;
- limitations;
- whether it conflicts with another source;
- whether it is current;
- whether it should affect the final design.

---

# 6. Build a candidate solution space

Do not jump from research to one architecture.

For every major problem, compare at least:

1. the simplest baseline;
2. the user's proposal;
3. the previous assistant proposal;
4. one strong alternative;
5. a hybrid;
6. “do nothing” or postpone.

Examples:

### For repeated work

- prompt;
- skill;
- agent-owned checklist;
- managed run;
- strict workflow.

### For semantic memory

- raw evidence;
- freeform notes;
- topic documents;
- open facets;
- typed views;
- strict operational records.

### For collaboration

- one agent;
- one agent with tools;
- one bounded subagent;
- independent attempts;
- manager-workers;
- full team.

### For a feature

- provider-native capability;
- adapter;
- prompt;
- plugin;
- subsystem;
- separate service.

---

# 7. Apply the “lowest sufficient layer” test

Before creating a new subsystem or separate document, test whether the requirement can be solved at a lower layer.

Use this order:

1. no change;
2. better prompt or instructions;
3. reusable skill/procedure;
4. provider-native capability;
5. lightweight adapter;
6. runtime record or guard;
7. reusable subsystem;
8. separate service or major document.

Choose the lowest layer that satisfies:

- reliability;
- observability;
- user control;
- portability;
- performance;
- safety.

A separate file is justified only if the domain has its own entities, lifecycle, ownership, and enough rules that distributing them would create inconsistency.

---

# 8. Simulate real scenarios

For each candidate architecture, mentally execute representative scenarios.

## 8.1. Normal path

Can an ordinary user complete the task without unnecessary ceremony?

## 8.2. Ambiguous request

Does the system ask at the right time, or create needless friction?

## 8.3. Strong model path

Does scaffolding help the model, or restrict capabilities it already has?

## 8.4. Weak model path

Does the system degrade safely?

## 8.5. Long-running path

Can it pause, resume, and survive restart?

## 8.6. External effect path

Can retries duplicate payment, email, deletion, publication, or configuration change?

## 8.7. Context-heavy path

Does decomposition destroy the global idea?

## 8.8. Multi-agent path

Do agents duplicate work, consume repeated context, or fight over mutable resources?

## 8.9. User control path

Can the user choose direct, balanced, independent, rigorous, or exploratory operation without disabling safety?

## 8.10. Failure and recovery

Can the system explain what failed, preserve partial artifacts, and continue without starting over?

## 8.11. Cost stress

Would a small quality improvement justify the extra agents, tokens, latency, and implementation?

## 8.12. Evolution

Can a derived component be replaced without losing canonical data or project history?

Document the result of scenario simulation as design consequences, not fictional dialogue.

---

# 9. Critique decomposition and multi-agent designs

Always test:

- Does each subtask produce an independently useful artifact?
- Can it be verified independently?
- Does it require most of the parent's context?
- Will the parent have to redo it to synthesize the result?
- Is shared mutable state involved?
- Is a coherent style or architecture required?
- Is parallelism actually available?
- Is model or source diversity real, or are the agents copies?
- What is the coordination tax?

Default to one strong agent when the task is tightly coupled.

Prefer independent full solutions over fragmented partial roles when preserving a unified idea matters.

Add reviewers only when objective checks are insufficient and the expected risk reduction justifies the cost.

---

# 10. Critique workflow designs

Distinguish:

- a prompt-defined procedure;
- a skill;
- an agent's temporary plan;
- a managed background run;
- a durable structured automation.

Do not require compilation, type checking, simulation, pilot, or visual graph for all of them.

Use strict workflows only when required by:

- durability;
- external effects;
- repeated use;
- mass parallelism;
- compliance;
- long waits;
- expensive execution;
- deterministic ordering.

Preserve model freedom inside agent-controlled phases.

---

# 11. Critique schemas and data models

Separate:

## Control envelope

Must be strict for:

- ID;
- owner;
- scope;
- time;
- permissions;
- provenance;
- sensitivity;
- lifecycle;
- external effects.

## Semantic payload

May remain open for:

- observations;
- taste;
- ideas;
- social patterns;
- research interpretation;
- evolving user behavior.

Introduce strict semantic structure only when repeated operations need exact state, validation, aggregation, or automation.

Prefer views and on-demand extraction over global migrations when possible.

---

# 12. Make decisions with acceptance and rejection gates

For every major component include:

## Problem

What failure or limitation it solves.

## Simplest baseline

What happens without it.

## Candidate mechanism

What is added.

## Expected benefit

Which end-task metric should improve.

## Cost

Tokens, latency, implementation, cognitive burden, maintenance.

## Failure modes

How the mechanism can make the system worse.

## Acceptance criteria

What evidence would justify it.

## Rejection criteria

When it must not be built or must be removed.

## Rollback/fallback

How the system continues without it.

Do not write “this is useful” without this analysis for major components.

---

# 13. Write the specification

## 13.1. Recommended structure

1. title, version, scope;
2. executive verdict;
3. what previous design got right and wrong;
4. research method;
5. immutable principles;
6. core entities;
7. lifecycle and business logic;
8. integration with adjacent systems;
9. user-control model;
10. failure and recovery;
11. observability and evaluation;
12. scenarios;
13. implementation order;
14. rejected alternatives;
15. source catalog;
16. final checklist.

## 13.2. Preserve adaptive areas

When behavior should remain model-driven, state principles, context, examples, boundaries, and evaluation — not a closed list of all future cases.

## 13.3. Formalize only operational contracts

Use state machines and schemas where they are necessary for reliable execution. Do not use them merely because the document looks more complete.

## 13.4. Make precedence explicit

When revising an existing document:

- state which version is current;
- identify superseded sections;
- preserve unaffected decisions;
- remove or neutralize contradictory language;
- include migration notes.

## 13.5. Avoid duplication

A concept should have one canonical owner document. Other files reference it rather than redefine it.

---

# 14. Validate the artifact

Before delivery perform all checks below.

## 14.1. Coverage

- every user requirement appears;
- every major scenario is covered;
- every critical entity has an owner and lifecycle;
- adjacent documents are connected;
- no requested area disappeared during simplification.

## 14.2. Contradiction audit

Search the document for old terminology and superseded claims.

Check especially:

- default vs optional;
- agent vs workflow;
- user vs orchestrator authority;
- memory vs permission;
- project vs task;
- historical vs current state;
- strict vs adaptive behavior.

## 14.3. Overengineering audit

For every subsystem ask:

- Could this be a prompt?
- Could this be a skill?
- Could this use a provider capability?
- Does it need its own service or file?
- Is its state actually durable?
- Does it justify its token and latency cost?

## 14.4. Underengineering audit

Also ask:

- Can data be lost?
- Can external effects repeat?
- Can permissions leak?
- Can concurrent writers conflict?
- Can the system recover?
- Can the result be verified?

Simplicity must not mean ignoring real invariants.

## 14.5. Source audit

- key factual claims are cited;
- sources actually support claims;
- primary sources dominate technical claims;
- current facts are verified;
- preprints are labeled appropriately;
- forum/issue evidence is not overstated.

## 14.6. File validation

- Markdown fences are balanced;
- headings are consistent;
- links are valid when checkable;
- version/date/status are correct;
- output file exists;
- unaffected source files were not overwritten unless requested.

---

# 15. Final response behavior

The chat response should:

1. link the artifact;
2. explain the central design decision;
3. state which user hypotheses were confirmed, rejected, or partially confirmed;
4. explain the largest changes;
5. explain why the result is better in practice;
6. disclose unavailable sources or unresolved uncertainty;
7. avoid merely repeating the table of contents.

When web research was used, cite the most load-bearing claims in the response.

Do not claim that the result is proven globally. State where Denet-specific evaluation is still required.

---

# 16. Research ledger templates

## Hypothesis record

```markdown
### H-01: <hypothesis>

Why plausible:
- ...

Why it may fail:
- ...

Evidence required:
- ...

Acceptance:
- ...

Rejection:
- ...

Decision:
- accepted | partial | rejected | deferred
```

## Component decision

```markdown
## <component>

Problem:

Simplest baseline:

Selected design:

Why not simpler:

Why not more complex:

Costs:

Failure modes:

Acceptance tests:

Fallback:
```

## Source record

```markdown
- Source:
- Date:
- Type:
- Supports:
- Limitations:
- Conflicts:
- Design impact:
```

## Scenario record

```markdown
### Scenario

Initial state:
Trigger:
Actors:
Expected flow:
Authoritative state:
User-visible behavior:
Failure:
Recovery:
Cost concern:
Acceptance:
```

---

# 17. Definition of done

The work is complete only when:

- the output is not merely a literature survey;
- the user's ideas have been tested rather than echoed;
- previous assistant ideas have been criticized equally;
- alternatives were compared;
- complexity has explicit justification;
- adaptive and strict parts are clearly separated;
- cost and latency are first-class concerns;
- multi-agent and workflow designs have simple baselines;
- scenarios expose edge cases;
- major components have acceptance/rejection criteria;
- the artifact is coherent enough to serve as input to architecture or implementation;
- the final answer explains the result and links the file.
