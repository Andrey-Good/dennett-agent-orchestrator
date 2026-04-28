# Stage 7 CLI Integrated Flow Transcript

This transcript is generated from normalized CLI assertions in `tests/integration/stage7-cli-integrated-flow.test.ts`.

## Offline Boundary

- Runtime adapter: Codex App Server methods are mocked in-process.
- State: temporary SQLite database.
- Live providers/network: not used.
- Proof limit: this proves CLI wiring and durable local resume semantics, not live Codex runtime behavior.

## Flow

1. `$ dennett-agent-orchestrator builder agent.stage7.cli.integrated --request <offline-builder-request> --run-id run-stage7-builder --state-db <temp-state-db>`
   - exit: 0
   - stdout: operation=create; builder_run_id=run-stage7-builder; draft_revision.kind=draft; live_revision=null
   - stderr: <empty>
2. `$ dennett-agent-orchestrator register <builder-draft-agent-file> --state-db <temp-state-db>`
   - exit: 0
   - stdout: logical_agent_id=agent.stage7.cli.integrated; revision.kind=draft
3. `$ dennett-agent-orchestrator status agent.stage7.cli.integrated --state-db <temp-state-db>`
   - exit: 0
   - stdout: live_revision=null; draft_revisions=1
4. `$ dennett-agent-orchestrator deploy <builder-draft-agent-file> --state-db <temp-state-db>`
   - exit: 0
   - stdout: logical_agent_id=agent.stage7.cli.integrated; revision.kind=live; live_file_path=<live-agent-file>
5. `$ dennett-agent-orchestrator run-live agent.stage7.cli.integrated --param topic="Stage 7 CLI proof" --run-id run-stage7-cli --state-db <temp-state-db>`
   - exit: 1
   - stderr: Run ID: run-stage7-cli; Local resume remains available.; RUN_WAITING_FOR_USER
6. `$ dennett-agent-orchestrator reply <live-agent-file> --run-id run-stage7-cli --prompt-id stage7-approval --text "Approved through the offline CLI fixture." --state-db <temp-state-db>`
   - exit: 0
   - stdout: Prompt reply delivered.
7. `$ dennett-agent-orchestrator run-status --run-id run-stage7-cli --state-db <temp-state-db>`
   - exit: 0
   - stdout: run.status=waiting_for_user; pending_prompt.prompt_id=stage7-approval; pending_prompt.reply.delivery_status=delivered_live
8. `$ dennett-agent-orchestrator resume <live-agent-file> --run-id run-stage7-cli --state-db <temp-state-db>`
   - exit: 0
   - stderr: Run ID: run-stage7-cli
   - stdout: final output="Approved Stage 7 CLI proof after offline prompt reply."
