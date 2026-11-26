import fs from "node:fs";
import path from "node:path";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "../App";
import type { WindowControls } from "../api/bridge";
import type {
  AgentRecord,
  ArtifactRecord,
  CapabilityRecord,
  ConsolidationJob,
  MemoryClaim,
  MemoryConflict,
  MemoryDocument,
  MemoryNote,
  MessageRecord,
  PermissionRequestRecord,
  ProjectRecord,
  SessionRecord,
  TaskRecord,
} from "../api/contracts";
import { createDevHttpBridge } from "../api/devHttpBridge";
import { MockDesktopBridge } from "../api/mockBridge";
import { openSafeExternalUrl, sanitizeExternalUrl } from "../components/MarkdownRenderer";
import { parseShellArgs } from "../components/SettingsSheet";
import { buildPreservedAgentConfigPatch } from "../state/WorkspaceProvider";

function createTestWindowControls(): WindowControls {
  return {
    supported: true,
    minimize: vi.fn(async () => {}),
    toggleMaximize: vi.fn(async () => {}),
    close: vi.fn(async () => {}),
    startDragging: vi.fn(async () => {}),
    isMaximized: vi.fn(async () => false),
  };
}

function makeRichFixture() {
  const now = Date.now();
  const project: ProjectRecord = {
    project_id: "10000000-0000-4000-8000-00000000a001",
    name: "AuditCore",
    workspace_path: "A:\\Work\\audit-core",
    memory_scope: "project:audit-core",
    lifecycle_state: "active",
    pinned: false,
    created_at: now - 60_000,
  };
  const agent: AgentRecord = {
    agent_id: "20000000-0000-4000-8000-00000000a001",
    name: "Audit Agent",
    description: "Основной агент",
    current_config_version_id: "21000000-0000-4000-8000-00000000a001",
    current_config: {
      config_version_id: "21000000-0000-4000-8000-00000000a001",
      agent_id: "20000000-0000-4000-8000-00000000a001",
      version_number: 1,
      system_prompt: "Ты строгий инженер Dennett.",
      model_preference: "gemini-4-neon",
      thinking_level: "medium",
      allowed_memory_scopes: [project.memory_scope],
      workspace_write_policy: { mode: "allow_workspace" },
      autonomy_mode: "auto",
      default_budget: { max_turns: 20, max_wall_secs: 600, exceed_policy: "ask_approval" },
      attached_skills: ["rust-checker", "sql-auditor"],
      preferred_connection_id: "conn-antigravity-oauth",
      created_at: now - 60_000,
    },
    created_at: now - 60_000,
    updated_at: now - 60_000,
  };
  const session: SessionRecord = {
    session_id: "30000000-0000-4000-8000-00000000a001",
    project_id: project.project_id,
    agent_id: agent.agent_id,
    title: "Главная сессия",
    session_kind: "user_chat",
    status: "active",
    pinned: false,
    created_at: now - 50_000,
    updated_at: now - 5_000,
  };
  const userMsg: MessageRecord = {
    message_id: "50000000-0000-4000-8000-00000000a001",
    session_id: session.session_id,
    role: "user",
    content: "Проверь безопасность ссылок и покажи код",
    run_id: "40000000-0000-4000-8000-00000000a001",
    correlation_id: "corr-1",
    created_at: now - 40_000,
  };
  const assistantMsg: MessageRecord = {
    message_id: "50000000-0000-4000-8000-00000000a002",
    session_id: session.session_id,
    role: "assistant",
    content:
      "Вот безопасная [документация](https://dennett.example/docs) и [опасная](javascript:alert(1)).\n\n```ts\nconst x = 42;\n```",
    run_id: "40000000-0000-4000-8000-00000000a001",
    correlation_id: "corr-1",
    created_at: now - 35_000,
  };

  return { now, project, agent, session, userMsg, assistantMsg };
}

function collectSourceFiles(dir: string, exts: string[]): string[] {
  const results: string[] = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "__tests__" || entry.name === "node_modules") {
        continue;
      }
      results.push(...collectSourceFiles(full, exts));
    } else if (exts.some((ext) => entry.name.endsWith(ext))) {
      results.push(full);
    }
  }
  return results;
}

describe("Bucket A Audit Closure & Chat Fork (Н5)", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("A-UI-P0-1: switching model or autonomy mode NEVER wipes attached_skills, default_budget, or preferred_connection_id", async () => {
    const user = userEvent.setup();
    const fx = makeRichFixture();

    // Direct helper invariant check
    const patch = buildPreservedAgentConfigPatch(fx.agent, { model_preference: "gemini-4-carbon" });
    expect(patch.attached_skills).toEqual(["rust-checker", "sql-auditor"]);
    expect(patch.default_budget).toEqual(fx.agent.current_config.default_budget);
    expect(patch.preferred_connection_id).toBe("conn-antigravity-oauth");
    expect(patch.autonomy_mode).toBe("auto");

    // Full end-to-end UI check via Composer model + autonomy popovers
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [fx.project],
      agents: [fx.agent],
      sessions: [fx.session],
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    await screen.findByTestId("chat-composer");

    // Switch model
    await user.click(screen.getByTestId("model-selector-trigger"));
    await user.click(await screen.findByTestId("model-option-gemini-4-carbon"));

    await waitFor(() => {
      expect(bridge.agents[0]?.current_config.model_preference).toBe("gemini-4-carbon");
    });
    expect(bridge.agents[0]?.current_config.attached_skills).toEqual(["rust-checker", "sql-auditor"]);
    expect(bridge.agents[0]?.current_config.default_budget?.max_turns).toBe(20);
    expect(bridge.agents[0]?.current_config.preferred_connection_id).toBe("conn-antigravity-oauth");

    // Switch autonomy mode -> ONLY Авто and Полный доступ offered!
    await user.click(screen.getByTestId("access-selector-trigger"));
    const accessMenu = await screen.findByTestId("access-popover");
    expect(accessMenu).toHaveTextContent("Авто");
    expect(accessMenu).toHaveTextContent("Полный доступ");
    expect(accessMenu).not.toHaveTextContent("Спрашивать запись");
    expect(accessMenu).not.toHaveTextContent("Только чтение");
    await user.click(screen.getByTestId("autonomy-option-full_access"));

    await waitFor(() => {
      expect(bridge.agents[0]?.current_config.autonomy_mode).toBe("full_access");
    });
    expect(bridge.agents[0]?.current_config.attached_skills).toEqual(["rust-checker", "sql-auditor"]);
    expect(bridge.agents[0]?.current_config.default_budget?.max_turns).toBe(20);
    expect(bridge.agents[0]?.current_config.preferred_connection_id).toBe("conn-antigravity-oauth");
  });

  it("Н5 Chat Fork + MarkdownRenderer code copy & safe external links + Ctrl+F search + approval approve_task + Emergency Stop + @-file autocomplete", async () => {
    const user = userEvent.setup();
    const fx = makeRichFixture();
    const writeTextMock = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: writeTextMock },
    });
    const openSpy = vi.spyOn(window, "open").mockImplementation(() => null);

    // Direct URL sanitizer security checks
    expect(sanitizeExternalUrl("https://example.com/a?b=1")).toBe("https://example.com/a?b=1");
    expect(sanitizeExternalUrl("javascript:alert(1)")).toBeNull();
    expect(sanitizeExternalUrl("data:text/html,<script>1</script>")).toBeNull();
    expect(sanitizeExternalUrl("file:///etc/passwd")).toBeNull();
    expect(openSafeExternalUrl("javascript:alert(1)")).toBe(false);

    const pendingPerm: PermissionRequestRecord = {
      request_id: "85000000-0000-4000-8000-00000000a001",
      project_id: fx.project.project_id,
      session_id: fx.session.session_id,
      run_id: "40000000-0000-4000-8000-00000000a001",
      agent_id: fx.agent.agent_id,
      tool_call_id: "tc-a1",
      tool_name: "shell_exec",
      tool_kind: "shell",
      target_summary: "curl https://api.example/pay",
      normalized_targets: ["curl https://api.example/pay"],
      parameters_sha256: "f".repeat(64),
      reason: "Критичное внешнее действие требует подтверждения владельца",
      status: "pending",
      revision: 1,
      expires_at: fx.now + 300_000,
      correlation_id: "corr-p1",
      created_at: fx.now - 1_000,
    };

    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [fx.project],
      agents: [fx.agent],
      sessions: [fx.session],
      runningSessionIds: [fx.session.session_id],
      messagesBySession: { [fx.session.session_id]: [fx.userMsg, fx.assistantMsg] },
      permissionRequests: [pendingPerm],
      filesByProject: {
        [fx.project.project_id]: [
          { relative_path: "README.md", name: "README.md", kind: "file", size_bytes: 120, depth: 1 },
          { relative_path: "src/app.ts", name: "app.ts", kind: "file", size_bytes: 340, depth: 2 },
        ],
      },
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    await screen.findByTestId("chat-card");

    // 1. Safe external link opens via window.open(..., "_blank", "noopener,noreferrer") without navigating; javascript: is blocked
    const safeLink = await screen.findByTestId("safe-external-link");
    await user.click(safeLink);
    expect(openSpy).toHaveBeenCalledWith(
      "https://dennett.example/docs",
      "_blank",
      "noopener,noreferrer",
    );
    expect(screen.getByTestId("blocked-external-link")).toHaveTextContent("опасная");

    // 2. Per-code-block Copy button copies raw code with "Скопировано" feedback
    const codeCopyBtn = screen.getByTestId("code-block-copy-btn");
    await user.click(codeCopyBtn);
    expect(writeTextMock).toHaveBeenCalledWith("const x = 42;");
    expect(codeCopyBtn).toHaveTextContent("Скопировано");

    // 3. Inline approval offers "Разрешить в этом чате" (approve_task)
    await user.click(screen.getByTestId(`inline-approve-task-${pendingPerm.request_id}`));
    await waitFor(() => {
      expect(
        bridge.calls.some(
          (c) =>
            c.method === "resolvePermissionRequest" &&
            (c.args as { request: { decision: string } }).request.decision === "approve_task",
        ),
      ).toBe(true);
    });

    // 4. Global Emergency Stop pill with inline confirm calls emergencyStopAll()
    await user.click(screen.getByTestId("emergency-stop-trigger"));
    await user.click(await screen.findByTestId("emergency-stop-confirm-btn"));
    await waitFor(() => {
      expect(bridge.calls.some((c) => c.method === "emergencyStopAll")).toBe(true);
    });

    // 5. Ctrl+F opens in-chat search bar, counts matches, Esc closes
    fireEvent.keyDown(window, { key: "f", ctrlKey: true });
    const chatSearchInput = await screen.findByTestId("chat-search-input");
    await user.type(chatSearchInput, "документация");
    expect(screen.getByTestId("chat-search-count")).toHaveTextContent("1 / 1");
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("chat-search-bar")).not.toBeInTheDocument();

    // 6. @-file autocomplete inserts @README.md cleanly
    const composer = screen.getByLabelText("Поле ввода сообщения") as HTMLTextAreaElement;
    await user.type(composer, "Посмотри @READ");
    const atPopover = await screen.findByTestId("at-file-popover");
    expect(atPopover).toHaveTextContent("README.md");
    await user.keyboard("{Enter}");
    expect(composer.value).toContain("@README.md ");

    // 7. Chat Fork (Н5): clicking "Ответвить чат" calls forkSession with from_message_id and switches session!
    await user.click(screen.getByTestId(`message-fork-btn-${fx.assistantMsg.message_id}`));
    await waitFor(() => {
      const forkCall = bridge.calls.find((c) => c.method === "forkSession");
      expect(forkCall).toBeTruthy();
      expect(
        (forkCall?.args as { request: { from_message_id: string } }).request.from_message_id,
      ).toBe(fx.assistantMsg.message_id);
      expect(bridge.sessions.length).toBe(2);
    });
  });

  it("A-UI-P0-3: Artifact immutable /content snapshot preview + Blob download, and clickable Workspace file preview + insert @path", async () => {
    const user = userEvent.setup();
    const fx = makeRichFixture();
    const art: ArtifactRecord = {
      artifact_id: "70000000-0000-4000-8000-00000000a001",
      session_id: fx.session.session_id,
      project_id: fx.project.project_id,
      name: "snapshot-plan.md",
      mime_type: "text/markdown",
      revision: 1,
      content: null, // Force loading from /content endpoint!
      sha256: "c".repeat(64),
      completeness_status: "final",
      lifecycle_state: "active",
      created_at: fx.now - 3_000,
    };

    const createObjUrlSpy = vi.fn(() => "blob:mock-artifact");
    const revokeObjUrlSpy = vi.fn();
    vi.stubGlobal("URL", {
      ...URL,
      createObjectURL: createObjUrlSpy,
      revokeObjectURL: revokeObjUrlSpy,
    });
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});

    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [fx.project],
      agents: [fx.agent],
      sessions: [fx.session],
      artifactsBySession: { [fx.session.session_id]: [art] },
      artifactContentsById: {
        [art.artifact_id]: {
          mime_type: "text/markdown",
          sha256: "d".repeat(64),
          byte_size: 28,
          text: "# Снимок артефакта из /content",
          base64: window.btoa("snapshot"),
          is_binary: false,
        },
      },
      filesByProject: {
        [fx.project.project_id]: [
          { relative_path: "src/lib.rs", name: "lib.rs", kind: "file", size_bytes: 48, depth: 2 },
        ],
      },
      fileContentsByProject: {
        [fx.project.project_id]: {
          "src/lib.rs": {
            relative_path: "src/lib.rs",
            mime_type: "text/plain",
            byte_size: 48,
            sha256: "e".repeat(64),
            content: "pub fn hello() -> &'static str { \"ok\" }",
            truncated: false,
            is_binary: false,
          },
        },
      },
    });

    render(
      <App
        bridge={bridge}
        windowControls={createTestWindowControls()}
        initialRightPanelOpen={true}
      />,
    );

    // 1. Artifact /content fetched & rendered + download button works
    const mdPreview = await screen.findByTestId("artifact-preview-markdown");
    expect(mdPreview).toHaveTextContent("Снимок артефакта из /content");
    expect(screen.getByTestId("artifact-sha256")).toHaveTextContent("dddddddddddd");

    await user.click(screen.getByTestId("artifact-download-btn"));
    expect(createObjUrlSpy).toHaveBeenCalledTimes(1);

    // 2. Clickable workspace file loads content preview & inserts @src/lib.rs into Composer
    await user.click(screen.getByTestId("workspace-file-src/lib.rs"));
    const fileContent = await screen.findByTestId("workspace-file-content");
    expect(fileContent).toHaveTextContent('pub fn hello() -> &\'static str { "ok" }');
    await user.click(screen.getByTestId("workspace-file-insert-chat"));

    const composer = screen.getByLabelText("Поле ввода сообщения") as HTMLTextAreaElement;
    expect(composer.value).toContain("@src/lib.rs");
  });

  it("A-UI-P0-2 & Settings tabs: candidate_unattached attach, restricted Trust/Revoke, SKILL.md viewer, 4-mode activation, quoted MCP args + custom env var, General prefs, Agent rollback & Policy rules, Diagnostics index status", async () => {
    const user = userEvent.setup();
    const fx = makeRichFixture();

    // Direct quoted-args parser check
    expect(parseShellArgs('-y --dir "My Docs" \'single arg\' plain')).toEqual([
      "-y",
      "--dir",
      "My Docs",
      "single arg",
      "plain",
    ]);

    const candidateCap: CapabilityRecord = {
      capability_id: "cap-cand-gemini",
      slug: "gemini-reviewer",
      kind: "skill",
      display_name: "Gemini Reviewer",
      description: "Кандидат из ~/.gemini",
      version: "1.0.0",
      origin_kind: "personal_discovered",
      ownership: "user",
      scope_kind: "candidate_unattached",
      project_id: null,
      possession_state: "discovered",
      trust_state: "restricted",
      activation_state: "disabled",
      compatibility_status: "compatible",
      source_path: "~/.gemini/skills/gemini-reviewer",
      content_sha256: "f".repeat(64),
      spec: {
        kind: "skill",
        skill_markdown: "## Инструкция навыка\nПроверяй инварианты перед коммитом.",
        has_scripts: false,
      },
      can_rollback: false,
      revision: 1,
      created_at: fx.now - 5_000,
      updated_at: fx.now - 5_000,
    };

    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [fx.project],
      agents: [fx.agent],
      sessions: [fx.session],
      capabilities: [candidateCap],
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    await screen.findByTestId("chat-card");

    // Open Settings via Ctrl+,
    fireEvent.keyDown(window, { key: ",", ctrlKey: true });
    expect(await screen.findByTestId("settings-sheet")).toBeInTheDocument();

    // 1. General tab: updatePreferences (enter_to_send + timezone)
    await user.click(screen.getByTestId("settings-tab-general"));
    await user.click(screen.getByTestId("settings-enter-send-false"));
    await waitFor(() => {
      expect(bridge.preferences.enter_to_send).toBe(false);
    });
    await user.click(screen.getByTestId("settings-timezone-trigger"));
    await user.click(await screen.findByTestId("settings-tz-option-Europe/Moscow"));
    await waitFor(() => {
      expect(bridge.preferences.timezone).toBe("Europe/Moscow");
    });

    // 2. Capabilities tab: inspect SKILL.md, trust restricted cap, attach candidate_unattached, 4-mode activation, MCP quoted args + env var
    await user.click(screen.getByTestId("settings-tab-capabilities"));
    await user.click(screen.getByTestId(`cap-inspect-btn-${candidateCap.capability_id}`));
    expect(
      await screen.findByTestId(`cap-manifest-viewer-${candidateCap.capability_id}`),
    ).toHaveTextContent("Проверяй инварианты перед коммитом.");

    // Trust restricted capability (no dead-end!)
    await user.click(screen.getByTestId(`cap-trust-btn-${candidateCap.capability_id}`));
    await waitFor(() => {
      expect(screen.getByTestId(`cap-trust-badge-${candidateCap.capability_id}`)).toHaveTextContent(
        "Доверенный",
      );
    });

    // Activating candidate_unattached promotes scope_kind to "project" (no unattached ghost!)
    await user.click(screen.getByTestId(`cap-act-${candidateCap.capability_id}-pinned`));
    await waitFor(() => {
      const updated = bridge.capabilities.find((c) => c.capability_id === candidateCap.capability_id);
      expect(updated?.activation_state).toBe("pinned");
      expect(updated?.scope_kind).toBe("project");
      expect(updated?.project_id).toBe(fx.project.project_id);
    });

    // MCP form: quoted args + custom env var name
    await user.click(screen.getByTestId("cap-toggle-mcp-btn"));
    await user.type(screen.getByTestId("cap-mcp-name-input"), "FS Server");
    await user.type(screen.getByTestId("cap-mcp-command-input"), "npx");
    await user.type(screen.getByTestId("cap-mcp-args-input"), '-y --root "My Project"');
    await user.type(screen.getByTestId("cap-mcp-secret-env-input"), "CUSTOM_API_TOKEN");
    await user.type(screen.getByTestId("cap-mcp-secret-slot-input"), "fs_mcp_secret");
    await user.type(screen.getByTestId("cap-mcp-secret-value-input"), "raw-token-999");
    await user.click(screen.getByTestId("cap-mcp-submit-btn"));

    await waitFor(() => {
      const mcp = bridge.capabilities.find((c) => c.display_name === "FS Server");
      expect(mcp?.spec.kind).toBe("mcp_server");
      if (mcp?.spec.kind === "mcp_server") {
        expect(mcp.spec.args).toEqual(["-y", "--root", "My Project"]);
        expect(mcp.spec.secret_slots).toEqual({ CUSTOM_API_TOKEN: "fs_mcp_secret" });
      }
    });

    // Providers tab shows ALL secrets including the custom MCP secret slot
    await user.click(screen.getByTestId("settings-tab-providers"));
    expect(await screen.findByTestId("secret-row-fs_mcp_secret")).toBeInTheDocument();

    // 3. Agent & Policies tab: edit prompt, create policy rule, delete policy rule
    await user.click(screen.getByTestId("settings-tab-agent"));
    const promptInput = await screen.findByTestId("settings-agent-prompt-input");
    await user.clear(promptInput);
    await user.type(promptInput, "Обновлённый промпт v2");
    await user.click(screen.getByTestId("settings-agent-save-btn"));
    await waitFor(() => {
      expect(bridge.agents[0]?.current_config.version_number).toBe(2);
    });

    await user.type(screen.getByTestId("policy-tool-input"), "shell_exec");
    await user.type(screen.getByTestId("policy-reason-input"), "Разрешить локальные тесты");
    await user.click(screen.getByTestId("policy-create-submit"));
    await waitFor(() => {
      expect(bridge.policyRules).toHaveLength(1);
    });
    const ruleId = bridge.policyRules[0]!.rule_id;
    await user.click(screen.getByTestId(`policy-delete-${ruleId}`));
    await waitFor(() => {
      expect(bridge.policyRules).toHaveLength(0);
    });

    // 4. Diagnostics tab shows memory index status card with localized Russian labels
    await user.click(screen.getByTestId("settings-tab-diagnostics"));
    const indexCard = await screen.findByTestId("diagnostics-memory-index-card");
    expect(indexCard).toHaveTextContent("Работает");
    expect(indexCard).toHaveTextContent("Позиция журнала");
    expect(indexCard).not.toHaveTextContent("Watermark seq");
  });

  it("Sidebar collapsible project groups & chat context menu + TasksCard RunNow/Pause/Resume/Delete/Subagents + MemoryCard TitleBar search, inline row forget, accept-both conflict & consolidation pause/resume", async () => {
    const user = userEvent.setup();
    const fx = makeRichFixture();

    const parentTask: TaskRecord = {
      task_id: "75000000-0000-4000-8000-00000000a001",
      project_id: fx.project.project_id,
      session_id: fx.session.session_id,
      agent_id: fx.agent.agent_id,
      title: "Родительское поручение",
      goal: "Провести аудит",
      generation: 1,
      status: "waiting_timer",
      schedule: {
        timezone: "UTC",
        kind: { kind: "interval", interval_secs: 3600 },
        next_fire_at: fx.now + 3600_000,
      },
      budget: { max_turns: 8 },
      usage: { turns_used: 1, tokens_used: 100, wall_ms_used: 500, confidence: "provider_reported" },
      attached_skills: [],
      created_at: fx.now - 10_000,
      updated_at: fx.now - 5_000,
    };
    const childTask: TaskRecord = {
      ...parentTask,
      task_id: "75000000-0000-4000-8000-00000000a002",
      parent_task_id: parentTask.task_id,
      title: "Дочерний подагент",
      goal: "Проверить тесты",
      schedule: null,
      status: "queued",
    };

    const noteA: MemoryNote = {
      note_id: "note-a1",
      scope: fx.project.memory_scope,
      current_revision: 1,
      title: "Квантовый ключ",
      body: "Используем **PostgreSQL** и pgvector.",
      tags: ["db"],
      created_at: fx.now - 5_000,
      updated_at: fx.now - 5_000,
    };
    const claimA: MemoryClaim = {
      claim_id: "claim-a1",
      scope: fx.project.memory_scope,
      topic_key: "search:target",
      natural_language: "Квантовый индекс включён в prod",
      epistemic_status: "asserted",
      source_event_id: "e1",
      created_at: fx.now - 4_000,
    };
    const claimB: MemoryClaim = {
      claim_id: "claim-a2",
      scope: fx.project.memory_scope,
      topic_key: "search:target",
      natural_language: "Квантовый индекс выключен в dev",
      epistemic_status: "contested",
      source_event_id: "e2",
      created_at: fx.now - 3_500,
    };
    const conflictA: MemoryConflict = {
      conflict_id: "conf-a1",
      scope: fx.project.memory_scope,
      topic_key: "search:target",
      conflict_type: "semantic",
      candidate_claim_ids: [claimA.claim_id, claimB.claim_id],
      status: "open",
      detected_at: fx.now - 3_000,
    };
    const docA: MemoryDocument = {
      document_id: "doc-a1",
      scope: fx.project.memory_scope,
      document_key: "dossier:quantum",
      title: "Квантовое досье",
      current_version: 1,
      current_body: "# Квантовый поиск\nВсё работает.",
      principal_id: "owner",
      created_at: fx.now - 2_000,
      updated_at: fx.now - 2_000,
    };
    const jobA: ConsolidationJob = {
      job_id: "job-a1",
      scope: fx.project.memory_scope,
      job_type: "dossier_update",
      status: "running",
      caveats_preserved: true,
      created_at: fx.now - 1_500,
      updated_at: fx.now - 1_000,
    };

    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [fx.project],
      agents: [fx.agent],
      sessions: [fx.session],
      tasks: [parentTask, childTask],
      notesByScope: { [fx.project.memory_scope]: [noteA] },
      memoryClaims: [claimA, claimB],
      memoryConflicts: [conflictA],
      memoryDocuments: [docA],
      consolidationJobs: [jobA],
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    await screen.findByTestId("chat-card");

    // 1. Sidebar collapsible project group + session context menu (archive & restore)
    await user.click(screen.getByTestId(`project-collapse-btn-${fx.project.project_id}`));
    expect(screen.queryByTestId(`session-item-${fx.session.session_id}`)).not.toBeInTheDocument();
    await user.click(screen.getByTestId(`project-collapse-btn-${fx.project.project_id}`));
    expect(screen.getByTestId(`session-item-${fx.session.session_id}`)).toBeInTheDocument();

    await user.click(screen.getByTestId(`session-menu-trigger-${fx.session.session_id}`));
    expect(await screen.findByTestId(`session-menu-export-${fx.session.session_id}`)).toBeInTheDocument();
    await user.click(screen.getByTestId(`session-menu-archive-${fx.session.session_id}`));
    await waitFor(() => {
      expect(bridge.sessions[0]?.status).toBe("archived");
    });

    // 2. TasksCard: subagent depth, Pause -> Resume, RunNow, inline delete
    await user.click(screen.getByTestId("rail-nav-tasks"));
    const childCard = await screen.findByTestId(`task-card-${childTask.task_id}`);
    expect(childCard).toHaveAttribute("data-subagent-depth", "1");
    expect(within(childCard).getByTestId(`task-subagent-badge-${childTask.task_id}`)).toHaveTextContent(
      "Подагент",
    );

    await user.click(screen.getByTestId(`task-pause-btn-${parentTask.task_id}`));
    await waitFor(() => {
      expect(bridge.calls.some((c) => c.method === "pauseTask")).toBe(true);
    });
    await user.click(await screen.findByTestId(`task-resume-btn-${parentTask.task_id}`));
    await waitFor(() => {
      expect(bridge.calls.some((c) => c.method === "resumeTask")).toBe(true);
    });
    await user.click(screen.getByTestId(`task-run-now-btn-${childTask.task_id}`));
    await waitFor(() => {
      expect(bridge.calls.some((c) => c.method === "runTaskNow")).toBe(true);
    });

    await user.click(screen.getByTestId(`task-delete-btn-${childTask.task_id}`));
    await user.click(await screen.findByTestId(`task-delete-confirm-yes-${childTask.task_id}`));
    await waitFor(() => {
      expect(screen.queryByTestId(`task-card-${childTask.task_id}`)).not.toBeInTheDocument();
    });

    // 3. MemoryCard: TitleBar search filters memory, inline Forget confirm inside clicked row,
    // Consolidation pause/resume, Conflict "Принять оба (контекстуально)"
    await user.click(screen.getByTestId("rail-nav-memory"));
    await screen.findByTestId("memory-card");

    // Consolidation job pause -> resume
    await user.click(screen.getByTestId(`consolidation-job-pause-${jobA.job_id}`));
    await waitFor(() => {
      expect(screen.getByTestId(`consolidation-job-status-${jobA.job_id}`)).toHaveTextContent(
        "на паузе",
      );
    });
    await user.click(screen.getByTestId(`consolidation-job-resume-${jobA.job_id}`));
    await waitFor(() => {
      expect(screen.getByTestId(`consolidation-job-status-${jobA.job_id}`)).toHaveTextContent(
        "завершено",
      );
    });

    // TitleBar search triggers unified memory search
    const topSearch = screen.getByLabelText("Поиск по чатам и проектам");
    await user.type(topSearch, "Квантовый");
    await waitFor(() => {
      expect(
        bridge.calls.some(
          (c) =>
            c.method === "searchMemory" &&
            (c.args as { query?: string })?.query === "Квантовый",
        ),
      ).toBe(true);
    });
    expect(screen.getByTestId(`memory-note-${noteA.note_id}`)).toBeInTheDocument();
    expect(screen.getByTestId(`memory-claim-${claimA.claim_id}`)).toBeInTheDocument();

    // Inline Forget confirmation appears DIRECTLY inside the clicked note card!
    const noteCard = screen.getByTestId(`memory-note-${noteA.note_id}`);
    await user.click(within(noteCard).getByTestId(`memory-note-forget-btn-${noteA.note_id}`));
    expect(within(noteCard).getByTestId("memory-forget-confirm")).toBeInTheDocument();

    // Conflict resolution: "Принять оба (контекстуально)" sends target_status="accepted_ambiguity"
    await user.click(screen.getByTestId("memory-tab-conflicts"));
    await user.click(await screen.findByTestId(`conflict-accept-both-${conflictA.conflict_id}`));
    await waitFor(() => {
      const resolveCall = bridge.calls.find((c) => c.method === "resolveMemoryConflict");
      expect(resolveCall).toBeTruthy();
      expect(
        (resolveCall?.args as { target_status?: string; winning_claim_id?: string | null })
          ?.target_status,
      ).toBe("accepted_ambiguity");
      expect(
        (resolveCall?.args as { winning_claim_id?: string | null })?.winning_claim_id,
      ).toBeNull();
    });
  });

  it("devHttpBridge sends Authorization Bearer token from window.__DENNETT_HEAD_TOKEN__ and routes all Stage 0 endpoints", async () => {
    (window as unknown as { __DENNETT_HEAD_TOKEN__?: string }).__DENNETT_HEAD_TOKEN__ =
      "owner-secret-token-42";

    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const method = init?.method ?? "GET";
      if (url.includes("/artifacts/") && url.endsWith("/content")) {
        return new Response("hello-artifact", {
          status: 200,
          headers: {
            "Content-Type": "text/plain",
            "X-Dennett-Artifact-Sha256": "a".repeat(64),
          },
        });
      }
      if (method === "DELETE") {
        return new Response(null, { status: 204 });
      }
      return new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    });
    vi.stubGlobal("fetch", fetchMock);

    try {
      const http = createDevHttpBridge("/__dennett_dev_head");
      await http.getArtifactContent("s1", "a1");
      await http.forkSession("s1", { from_message_id: "m1" });
      await http.deleteSession("s1");
      await http.getPreferences();
      await http.updatePreferences({ timezone: "UTC", enter_to_send: false });
      await http.getProjectFileContent("p1", "src/main.rs");
      await http.getAgentConfigHistory("ag1");
      await http.listPolicyRules();
      await http.createPolicyRule({
        scope: "global",
        tool_name: "shell_exec",
        effect: "allow",
        reason: "ok",
      });
      await http.deletePolicyRule("r1");
      await http.emergencyStopAll();
      await http.runTaskNow("t1");
      await http.pauseTask("t1");
      await http.resumeTask("t1");
      await http.deleteTask("t1");
      await http.getMemoryIndexStatus("project:p1");

      expect(fetchMock.mock.calls.length).toBe(16);
      for (const [, init] of fetchMock.mock.calls) {
        const headers = new Headers(init?.headers);
        expect(headers.get("Authorization")).toBe("Bearer owner-secret-token-42");
      }
      expect(
        fetchMock.mock.calls.map(([u, init]) => `${init?.method ?? "GET"} ${String(u)}`),
      ).toEqual([
        "GET /__dennett_dev_head/v1/sessions/s1/artifacts/a1/content",
        "POST /__dennett_dev_head/v1/sessions/s1/fork",
        "DELETE /__dennett_dev_head/v1/sessions/s1",
        "GET /__dennett_dev_head/v1/preferences",
        "PATCH /__dennett_dev_head/v1/preferences",
        "GET /__dennett_dev_head/v1/projects/p1/file?path=src%2Fmain.rs",
        "GET /__dennett_dev_head/v1/agents/ag1/config-versions",
        "GET /__dennett_dev_head/v1/policies",
        "POST /__dennett_dev_head/v1/policies",
        "DELETE /__dennett_dev_head/v1/policies/r1",
        "POST /__dennett_dev_head/v1/runs/emergency-stop",
        "POST /__dennett_dev_head/v1/tasks/t1/run-now",
        "POST /__dennett_dev_head/v1/tasks/t1/pause",
        "POST /__dennett_dev_head/v1/tasks/t1/resume",
        "DELETE /__dennett_dev_head/v1/tasks/t1",
        "GET /__dennett_dev_head/v1/memory/index/status?scope=project%3Ap1",
      ]);
    } finally {
      delete (window as unknown as { __DENNETT_HEAD_TOKEN__?: string }).__DENNETT_HEAD_TOKEN__;
    }
  });

  it("Static Guardrails: ZERO <select>, ZERO backdrop-filter, ZERO dangerouslySetInnerHTML, ZERO window.confirm/alert/prompt across all desktop source files", () => {
    const srcRoot = path.resolve(__dirname, "..");
    const tsFiles = collectSourceFiles(srcRoot, [".ts", ".tsx"]);
    const cssFiles = collectSourceFiles(srcRoot, [".css"]);

    expect(tsFiles.length).toBeGreaterThan(10);
    expect(cssFiles.length).toBeGreaterThan(0);

    for (const file of tsFiles) {
      const raw = fs.readFileSync(file, "utf8");
      const content = raw
        .replace(/\/\*[\s\S]*?\*\//g, "")
        .replace(/\/\/[^\n]*/g, "");
      const rel = path.relative(srcRoot, file);
      expect(content, `${rel} must not contain <select>`).not.toMatch(/<select\b/i);
      expect(content, `${rel} must not contain dangerouslySetInnerHTML`).not.toContain(
        "dangerouslySetInnerHTML",
      );
      expect(content, `${rel} must not call window.confirm/alert/prompt`).not.toMatch(
        /\bwindow\.(confirm|alert|prompt)\s*\(/,
      );
    }

    for (const file of cssFiles) {
      const raw = fs.readFileSync(file, "utf8");
      const withoutComments = raw.replace(/\/\*[\s\S]*?\*\//g, "");
      const rel = path.relative(srcRoot, file);
      expect(withoutComments, `${rel} must not use backdrop-filter`).not.toMatch(
        /backdrop-filter\s*:/i,
      );
    }
  });
});

describe("D3 Post-Audit Polish & Visual Closure", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  });

  it("P2-1: @file failure does NOT loop and renders inline Russian error inside @ popover", async () => {
    const user = userEvent.setup();
    const fx = makeRichFixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [fx.project],
      agents: [fx.agent],
      sessions: [fx.session],
    });
    const listFilesSpy = vi.fn(async () => {
      throw { code: "WORKSPACE_ERROR", message: "Permission denied" };
    });
    bridge.listProjectFiles = listFilesSpy;

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    const composer = (await screen.findByLabelText("Поле ввода сообщения")) as HTMLTextAreaElement;

    await user.type(composer, "Открой @src");
    const errorEl = await screen.findByTestId("at-file-error");
    expect(errorEl).toHaveTextContent("Не удалось прочитать папку проекта");

    // Wait briefly to guarantee Composer's useEffect does NOT enter an infinite retry loop.
    await new Promise((resolve) => window.setTimeout(resolve, 120));
    expect(listFilesSpy).toHaveBeenCalledTimes(1);

    // Escape closes the @ popover cleanly even in error state.
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("at-file-popover")).not.toBeInTheDocument();
  });

  it("P1-5: editing message #0 creates a clean sibling branch without duplicating in original chat and shows return bar", async () => {
    const user = userEvent.setup();
    const fx = makeRichFixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [fx.project],
      agents: [fx.agent],
      sessions: [fx.session],
      messagesBySession: { [fx.session.session_id]: [fx.userMsg, fx.assistantMsg] },
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    await screen.findByTestId(`message-edit-btn-${fx.userMsg.message_id}`);

    // Put a draft in the original composer to verify drafts are cleared cleanly on edit & resend.
    const composer = screen.getByLabelText("Поле ввода сообщения") as HTMLTextAreaElement;
    await user.type(composer, "старый черновик");
    expect(composer.value).toBe("старый черновик");

    // Edit the very first message (idx === 0)
    await user.click(screen.getByTestId(`message-edit-btn-${fx.userMsg.message_id}`));
    const editForm = await screen.findByTestId(`message-edit-form-${fx.userMsg.message_id}`);
    const editTextarea = within(editForm).getByLabelText("Редактировать сообщение");
    await user.clear(editTextarea);
    await user.type(editTextarea, "Обновлённый первый запрос");
    await user.click(screen.getByTestId(`message-edit-send-${fx.userMsg.message_id}`));

    // Must create a sibling branch session rather than resending duplicate into original chat!
    await waitFor(() => {
      expect(bridge.calls.some((c) => c.method === "createSession")).toBe(true);
      expect(bridge.sessions).toHaveLength(2);
    });
    expect(bridge.calls.some((c) => c.method === "forkSession")).toBe(false);

    // Original session retains its exact 2 messages (no duplicate sent into original chat)
    expect(bridge.messagesBySession[fx.session.session_id]).toHaveLength(2);
    const branchSession = bridge.sessions.find((s) => s.session_id !== fx.session.session_id);
    expect(branchSession).toBeTruthy();
    expect(branchSession?.title).toContain("(ветка)");

    // Edited prompt was dispatched to the new branch session and composer draft is empty
    const sendCalls = bridge.calls.filter((c) => c.method === "sendMessage");
    expect(sendCalls).toHaveLength(1);
    expect((sendCalls[0]?.args as { sessionId: string }).sessionId).toBe(branchSession?.session_id);
    expect((screen.getByLabelText("Поле ввода сообщения") as HTMLTextAreaElement).value).toBe("");

    // Sleek fork-origin bar appears with 1-click return link to original chat
    const notice = await screen.findByTestId("fork-origin-notice");
    expect(notice).toHaveTextContent("Создана ветка от сообщения");
    await user.click(screen.getByTestId("fork-origin-return-btn"));

    await waitFor(() => {
      expect(screen.queryByTestId("fork-origin-notice")).not.toBeInTheDocument();
      expect(screen.getByText(fx.userMsg.content)).toBeInTheDocument();
    });
    // Original session composer draft was also cleared cleanly
    expect((screen.getByLabelText("Поле ввода сообщения") as HTMLTextAreaElement).value).toBe("");
  });

  it("P2-2: deleteSession never falls back to background task_run or archived sessions", async () => {
    const user = userEvent.setup();
    const fx = makeRichFixture();
    const taskRunSession: SessionRecord = {
      session_id: "30000000-0000-4000-8000-00000000a099",
      project_id: fx.project.project_id,
      agent_id: fx.agent.agent_id,
      title: "Фоновый запуск",
      session_kind: "task_run",
      status: "active",
      pinned: false,
      created_at: fx.now - 30_000,
      updated_at: fx.now - 2_000,
    };
    const archivedSession: SessionRecord = {
      session_id: "30000000-0000-4000-8000-00000000a098",
      project_id: fx.project.project_id,
      agent_id: fx.agent.agent_id,
      title: "Архивный чат",
      session_kind: "user_chat",
      status: "archived",
      pinned: false,
      created_at: fx.now - 20_000,
      updated_at: fx.now - 1_000,
    };
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [fx.project],
      agents: [fx.agent],
      sessions: [fx.session, taskRunSession, archivedSession],
      messagesBySession: { [fx.session.session_id]: [fx.userMsg] },
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    await screen.findByTestId("chat-composer");

    // Delete the sole active user_chat via sidebar menu
    await user.click(screen.getByTestId(`session-menu-trigger-${fx.session.session_id}`));
    await user.click(await screen.findByTestId(`session-menu-delete-${fx.session.session_id}`));
    await user.click(await screen.findByTestId(`session-menu-delete-yes-${fx.session.session_id}`));

    // Must land on clean empty project state (selectedSessionId === null), NEVER task_run or archived!
    await screen.findByTestId("empty-project-sessions-state");
    expect(screen.queryByTestId("composer-task-run-banner")).not.toBeInTheDocument();
    expect(screen.queryByTestId(`session-archived-badge-${archivedSession.session_id}`)).not.toBeInTheDocument();
  });

  it("Opus P1-2 & Carbon Visual Polish: honest Full Access copy, localized SettingsSheet diagnostics with zero Watermark seq, dark obsidian MCP toolbar button", async () => {
    const user = userEvent.setup();
    const fx = makeRichFixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      runtimeIsSimulated: true,
      projects: [fx.project],
      agents: [fx.agent],
      sessions: [fx.session],
    });
    const origDiagnostics = bridge.listDiagnostics.bind(bridge);
    bridge.listDiagnostics = async (limit?: number) => {
      const base = await origDiagnostics(limit);
      return {
        ...base,
        events: [
          {
            event_id: "ev-1",
            correlation_id: "c1",
            component: "SimulatedRuntime",
            event_kind: "run_succeeded",
            safe_summary: "SimulatedRuntime run_succeeded healthy",
            created_at: fx.now - 2_000,
          },
          {
            event_id: "ev-2",
            correlation_id: "c2",
            component: "head",
            event_kind: "run_failed",
            safe_summary: "runtime degraded unreachable",
            created_at: fx.now - 1_000,
          },
        ],
      };
    };

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    await screen.findByTestId("chat-composer");

    // 1. Composer Full Access copy is honest and accurate
    await user.click(screen.getByTestId("access-selector-trigger"));
    const accessMenu = await screen.findByTestId("access-popover");
    expect(accessMenu).toHaveTextContent(
      "Полная автономность без интерактивных подтверждений (уровень полного доступа)",
    );
    await user.keyboard("{Escape}");

    // 2. SettingsSheet: SimulatedRuntime localized, MCP button uses obsidian secondary class, zero Watermark seq
    fireEvent.keyDown(window, { key: ",", ctrlKey: true });
    const sheet = await screen.findByTestId("settings-sheet");
    expect(sheet).toHaveTextContent("Симуляция");
    expect(sheet).not.toHaveTextContent("SimulatedRuntime");

    await user.click(screen.getByTestId("settings-tab-capabilities"));
    const mcpBtn = await screen.findByTestId("cap-toggle-mcp-btn");
    expect(mcpBtn.className).toContain("popover-secondary-btn");
    expect(mcpBtn.className).not.toContain("popover-primary-btn");

    await user.click(screen.getByTestId("settings-tab-diagnostics"));
    const diagPanel = await screen.findByTestId("settings-diagnostics-panel");
    expect(diagPanel).toHaveTextContent("Позиция журнала");
    expect(diagPanel).not.toHaveTextContent("Watermark seq");
    expect(diagPanel).toHaveTextContent("Работает");
    expect(diagPanel).toHaveTextContent("Запуск завершён");
    expect(diagPanel).toHaveTextContent("Ошибка запуска");
    expect(diagPanel).toHaveTextContent("Деградация");
    expect(diagPanel).toHaveTextContent("Недоступен");
    expect(diagPanel).not.toHaveTextContent("run_succeeded");
    expect(diagPanel).not.toHaveTextContent("run_failed");
    expect(diagPanel).not.toHaveTextContent("SimulatedRuntime");
  });

  it("P1-4 & P2-4: Tauri openSafeExternalUrl invokes open_external_url without window.open, and Vite proxy uses server-side DENNETT_HEAD_TOKEN", async () => {
    const openSpy = vi.spyOn(window, "open").mockImplementation(() => null);
    const tauriInvokeSpy = vi.fn(async () => undefined);
    (
      window as unknown as {
        __TAURI_INTERNALS__?: { invoke: typeof tauriInvokeSpy };
      }
    ).__TAURI_INTERNALS__ = { invoke: tauriInvokeSpy };

    expect(openSafeExternalUrl("https://dennett.example/safe")).toBe(true);
    await waitFor(() => {
      expect(tauriInvokeSpy).toHaveBeenCalledWith(
        "open_external_url",
        { url: "https://dennett.example/safe" },
        undefined,
      );
    });
    expect(openSpy).not.toHaveBeenCalled();

    // Verify Vite dev proxy injects Authorization header server-side from DENNETT_HEAD_TOKEN
    const viteConfigSrc = fs.readFileSync(path.resolve(__dirname, "../../vite.config.ts"), "utf8");
    expect(viteConfigSrc).toContain("process.env.DENNETT_HEAD_TOKEN");
    expect(viteConfigSrc).toContain("Authorization: `Bearer ${devHeadToken}`");
  });
});
