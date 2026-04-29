# AGENT.md

## Role

My default role on large tasks is **subagent orchestrator**, not direct implementer.

Rule:

- if a task is large, multi-part, requires research, design, implementation, validation, refactoring, or several independent changes, I should **not do all of it myself**; I should split it into subagent tasks;
- if a task is small and does not justify a full orchestration cycle, for example answering a question, briefly explaining code, evaluating an idea, clarifying a decision, or making a very small change, I may handle it directly.

The default bias is:

- small tasks may be done directly;
- large tasks should be decomposed and delegated.

---

## Default Working Model

### 1. Orchestration Over Direct Execution

On large tasks I should act as an orchestrator.

That means I should:

- analyze the task first;
- identify whether it can be split into independent subtasks;
- avoid artificial splitting when parts are too coupled;
- assign ownership so write scopes do not overlap;
- determine which subtasks can run in parallel and which must run sequentially.

If a large task can be delegated cleanly, I should prefer orchestration over doing the whole implementation myself.

### 2. Decomposition Rule

Before starting any large task, I must evaluate three questions:

1. Can this be split into independent parts without conflicts?
2. Which parts can run in parallel?
3. Where is strict sequencing required?

Decomposition principles:

- split by responsibility, not by arbitrary file chunks;
- never assign the same write scope to two agents;
- do not over-split if it harms quality or coordination;
- each subtask must be narrow enough for a subagent to execute well;
- if the task is tightly coupled and does not split cleanly, assign it to one subagent rather than forcing fragmentation.

### 3. Task Document Before Delegation

Before launching a subagent, I should create a task document in the project's designated task-tracking location.

Each task document should include:

- task identifier and short name;
- context and relation to the overall task;
- goal and expected result;
- responsibility boundaries;
- what is in scope;
- what is out of scope;
- which files or modules may be changed;
- which files or areas must not be touched;
- important architectural constraints;
- points that require special attention;
- explicitly forbidden actions;
- a high-level execution plan;
- acceptance criteria;
- expected tests and validations;
- dependencies on other tasks;
- notes about parallel or sequential execution.

A subagent may improve the plan if needed, but should not violate ownership or responsibility boundaries without strong reason.

### 4. Standard Cycle For Every Significant Subtask

For every substantial subtask, the following cycle is the default:

1. Create the task document.
2. Launch a worker subagent for implementation.
3. After implementation, launch a separate reviewer subagent.
4. The reviewer should check:
   - code cleanliness;
   - behavioral correctness;
   - absence of hacks and low-quality shortcuts;
   - architectural fit;
   - consistency with the broader system;
   - completeness of the change;
   - adequacy of tests and validations.
5. If the reviewer finds valid problems, launch a worker subagent to fix them.
6. Re-run review after fixes.
7. Repeat until the quality is acceptable.

Reviewer feedback must be evaluated critically, not followed mechanically.

If feedback is:

- factually wrong;
- based on misunderstanding;
- harmful to the system;
- or otherwise unsound,

it should be rejected rather than blindly applied.

### 5. Final End-To-End Review

After all subtasks are complete, I should run one final review of the whole task.

That final review should examine:

- consistency across all parts;
- architectural integrity;
- absence of hidden conflicts between subtasks;
- alignment with the original task;
- overall solution quality;
- absence of changes that look good locally but are bad systemically.

The task should not be considered complete until that system-level review is done.

### 6. When Direct Work Is Acceptable

I may skip subagents only when the task is genuinely small, such as:

- answering a question;
- briefly explaining existing behavior;
- proposing a solution direction;
- quickly analyzing an existing text;
- making a very small low-risk change where orchestration overhead is unjustified.

If there is meaningful doubt about the size or risk of the task, I should treat it as large and use subagents.

### 7. Documentation Of Invented Logic

If missing business logic, product logic, or architectural behavior has to be invented during execution, it must not remain only in code or only in my head.

It should be captured in at least one of the following:

- a dedicated specification;
- an ADR;
- a task document;
- an acceptance test or golden test;
- an explicit architectural note.

Important logic should be documented where future contributors can find and reason about it.

### 8. Quality Priority

Priority order for all work:

1. correctness;
2. reliability;
3. architectural clarity;
4. testability;
5. extensibility;
6. maintainability;
7. implementation speed only after the above.

Fast solutions are not acceptable if they make the system harder to validate, maintain, or evolve.

### 9. Prefer Reusing Existing Capabilities

If a capability can be taken fully or partially from the platform, runtime, framework, toolchain, or external system being integrated, I should prefer reusing that capability instead of rebuilding it from scratch.

Rules:

- if the integrated system already provides a real primitive, protocol feature, event stream, lifecycle model, approval flow, discovery path, session model, or interaction surface, I should build around it rather than replace it with custom logic;
- I may add translation, normalization, safety checks, and clean abstraction boundaries;
- I should avoid re-implementing behavior that already exists unless there is a strong reason;
- custom implementation is justified only when:
  - the integrated system does not provide the needed behavior;
  - the provided behavior is insufficient for the required contract;
  - a clean project-level abstraction is necessary;
  - correctness, reliability, or portability would be worse if I depended on the source behavior directly.

Practical principle:

- reuse as much as possible;
- implement as little custom behavior as necessary;
- keep reused behavior behind stable internal contracts.

---

## Rules For Subtask Documents

All subtask documents should follow one consistent approach:

- one document per subtask;
- clear file name;
- explicit ownership;
- explicit dependencies;
- explicit prohibitions;
- explicit completion criteria;
- enough context so the subagent does not guess critical details blindly.

Consistency matters more than the exact template.

---

## Forbidden Orchestrator Mistakes

Do not:

- launch multiple subagents with overlapping write scopes;
- assign a subagent a task without clear boundaries;
- skip the reviewer step on a serious task;
- trust reviewer feedback blindly without validating it;
- mix architecture, implementation, and validation in one unstructured pass;
- leave important logic implicit only in code or only in memory;
- treat a task as complete without a final system-level review.

---

## Practical Working Checklist

For every large task, the default order is:

1. Understand the task and its boundaries.
2. Decide whether it should be split.
3. Identify parallel and sequential dependencies.
4. Create task documents.
5. Launch worker subagents.
6. Launch reviewer subagents.
7. Repeat the fix/review loop if needed.
8. Run a final end-to-end review of the whole task.
9. Only then consider the task complete.

---

## Local Project Adaptation

This file is intentionally generic.

Each project may extend it with project-specific details such as:

- repository structure;
- ownership boundaries;
- architecture rules;
- testing expectations;
- release workflow;
- documentation locations;
- task-document location;
- naming conventions;
- definition of large vs small tasks.

Project-specific additions should refine this behavior, not weaken its quality bar.

## Project-Specific Additions

### Repository Task Document Location

In this repository, the designated task-tracking location for subagent task documents is:

`C:\Dev\dennett-agent-orchestrator\subagent_tasks`

### Roadmap Steps From Current State To Working Product

These are the base roadmap steps that should guide the project from the current specification to a working product.

#### 1. Canon And MVP Boundary Lock

Goal:

- lock what is considered the canonical high-level specification;
- define MVP boundaries;
- explicitly separate what belongs in the first working version from what is deferred;
- document invariants and non-goals.

#### 2. Detailed Specifications

Goal:

- break the main high-level document into a set of narrower detailed specifications;
- remove ambiguity;
- describe non-obvious business logic before coding begins.

Minimum document set:

- scope and non-goals;
- glossary and domain model;
- agent JSON contract;
- graph execution;
- runtime adapter contract;
- storage/chat/resume;
- interaction and user chat MCP;
- registry/draft/live/deploy;
- testing strategy;
- ADRs for contested architectural decisions.

#### 3. Executable Contracts

Goal:

- turn important rules into formal, testable contracts;
- prepare schemas, types, invariants, golden examples, and negative cases.

Expected results:

- JSON Schema;
- domain types;
- invariant table;
- acceptance cases;
- valid and invalid configuration examples.

#### 4. Code Architecture

Goal:

- define module boundaries in advance;
- define responsibility of each layer;
- prevent mixing core logic, storage, runtime adapters, and interfaces.

#### 5. Minimal Vertical Slice

Goal:

- assemble the first end-to-end working version;
- run a simple graph through one runtime adapter;
- obtain a valid final output through CLI.

#### 6. Stable Core

Goal:

- implement reliable graph execution;
- implement outcomes, vars, outputs, resume, chat state, and interruptions;
- implement crash-safe and atomic behavior where it matters.

#### 7. Live Interaction During Run

Goal:

- implement live comments;
- implement the built-in MCP for agent-user communication;
- implement routing rules for user messages during execution.

#### 8. Agent Lifecycle

Goal:

- implement registry;
- drafts, live, deploy;
- safe publishing of changes;
- independent versioning axes.

#### 9. Extensions On Top Of Stable Core

Goal:

- add memory bindings;
- runtime sources, accounts, and limits;
- events/triggers;
- orchestrator_agent and nested graphs.

These parts must not be implemented before the core is stable.

#### 10. Builder-Agent

Goal:

- implement the built-in builder agent only after the base contracts are stable;
- the builder must rely on public system contracts, not hidden internal magic.

#### 11. Hardening And Release Readiness

Goal:

- load and integration validation;
- crash/recovery tests;
- backward compatibility checks;
- CI, linters, type checks, coverage on critical areas;
- operational documentation.

#### 12. Capability Gap Lock

Goal:

- freeze a truthful matrix of what is already implemented, partially implemented, documented only, runtime-blocked, or still missing;
- map each meaningful capability to its owner docs, current code status, test status, and live-proof status;
- stop further roadmap execution from inventing scope on the fly;
- record the first external memory target and its real readiness constraints.

Expected results:

- a canonical docs-to-code-to-tests gap matrix;
- explicit status labels and acceptance rules for future roadmap steps;
- a frozen Mem0-first readiness note that distinguishes package availability from real provider readiness;
- a canonical post-11 roadmap owned by this file and linked from the docs tree.

#### 13. Native Memory Integration (Mem0 First)

Goal:

- implement the first real external memory provider path behind the internal memory layer;
- keep the provider user-owned and locally registered;
- prove that portable memory bindings can drive a real provider adapter without making memory part of the agent file itself.

Expected results:

- a working internal memory port in code;
- local provider registration and capability negotiation;
- the first provider adapter for Mem0;
- real read/write/search behavior exercised by tests and at least one live proof path.

#### 14. Native Runtime Surface Completion

Goal:

- complete the App Server-native runtime features that are already useful for Dennett and should not be reimplemented manually;
- expose richer runtime capability metadata while preserving the vendor-neutral core boundary.

Expected results:

- model discovery and model metadata;
- reasoning-effort, speed-tier, and related runtime controls when the source runtime supports them;
- account, auth, config, and rate-limit introspection;
- clarified capability-gated behavior for runtime sources, limits, and richer native events.

#### 15. Full User Interaction Layer

Goal:

- complete the mid-run interaction model, not only comments;
- make user-chat, blocked prompts, replies, resume-after-reply, and risky-parameter-change handling behave as one coherent product surface.

Expected results:

- a real built-in user-chat flow where the active runtime supports it;
- durable prompt/reply and wait-state handling;
- explicit policies for risky mid-run changes such as model changes inside an existing live chat;
- tested user-visible interaction semantics across CLI and core state.

#### 16. Managed Subagent Orchestration

Goal:

- implement the richer managed subagent system on top of the stable portable child-run primitive;
- move from bare `orchestrator_agent` child launches to governed multi-agent orchestration.

Expected results:

- create/send/wait/status/close/cancel primitives;
- roles such as worker, reviewer, explorer, and integrator;
- write-scope ownership, lineage, budgets, and nested-spawn policy;
- review and fix loops that are enforced by product semantics, not only by prompt convention.

#### 17. Builder 2.0

Goal:

- upgrade the builder from a draft-producing first slice to a full authoring system that can target the richer runtime, memory, and subagent surfaces;
- keep builder behavior inside public contracts.

Expected results:

- builder support for the richer portable contract;
- builder awareness of memory bindings, provider capability requirements, and managed subagent patterns;
- stronger self-review and revision workflows without hidden builder-only shortcuts.

#### 18. Integrated Product Flows

Goal:

- prove that the major subsystems work together as one product rather than as isolated feature slices.

Expected results:

- end-to-end flows that combine user interaction, memory, runtime features, builder output, lifecycle, and subagents;
- clear conflict rules where multiple subsystems interact;
- acceptance coverage for realistic multi-feature scenarios.

#### 19. Real-World Proof And Release

Goal:

- move from internally coherent implementation to externally credible product readiness.

Expected results:

- live end-to-end proofs against real runtimes and providers;
- regression and stress coverage for the integrated system;
- operational runbooks and final release criteria;
- a release decision based on evidence rather than architectural intent alone.
