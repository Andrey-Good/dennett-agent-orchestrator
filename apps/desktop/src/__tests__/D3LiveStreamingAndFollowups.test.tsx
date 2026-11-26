import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { App } from "../App";
import type { WindowControls } from "../api/bridge";
import type {
  AgentRecord,
  ProjectRecord,
  RunRecord,
  SessionFollowupRecord,
  SessionRecord,
} from "../api/contracts";
import { createDevHttpBridge } from "../api/devHttpBridge";
import { MockDesktopBridge } from "../api/mockBridge";

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

function makeFixture() {
  const now = Date.now();
  const project: ProjectRecord = {
    project_id: "10000000-0000-4000-8000-000000000101",
    name: "Dennett Core",
    workspace_path: "A:\\Work\\dennett",
    memory_scope: "project:dennett-core",
    lifecycle_state: "active",
    pinned: false,
    created_at: now - 30_000,
  };
  const agent: AgentRecord = {
    agent_id: "20000000-0000-4000-8000-000000000101",
    name: "Проектный агент",
    description: "Основной агент",
    current_config_version_id: "21000000-0000-4000-8000-000000000101",
    current_config: {
      config_version_id: "21000000-0000-4000-8000-000000000101",
      agent_id: "20000000-0000-4000-8000-000000000101",
      version_number: 1,
      system_prompt: "Ты агент Dennett.",
      model_preference: "gemini-4-neon",
      thinking_level: "medium",
      allowed_memory_scopes: [project.memory_scope],
      workspace_write_policy: { mode: "allow_workspace" },
      created_at: now - 30_000,
    },
    created_at: now - 30_000,
    updated_at: now - 30_000,
  };
  const sessionA: SessionRecord = {
    session_id: "30000000-0000-4000-8000-000000000101",
    agent_id: agent.agent_id,
    project_id: project.project_id,
    title: "Живой диалог A",
    status: "active",
    pinned: false,
    created_at: now - 20_000,
    updated_at: now - 5_000,
  };
  const sessionB: SessionRecord = {
    session_id: "30000000-0000-4000-8000-000000000102",
    agent_id: agent.agent_id,
    project_id: project.project_id,
    title: "Спокойный диалог B",
    status: "active",
    pinned: false,
    created_at: now - 25_000,
    updated_at: now - 10_000,
  };
  return { now, project, agent, sessionA, sessionB };
}

afterEach(() => {
  cleanup();
  window.localStorage.clear();
  vi.unstubAllGlobals();
});

describe("D3+ Live Streaming, Follow-Up Queue, Live Steering & Context Usage", () => {
  it("renders clean live Markdown streaming bubble with safe skipHtml and zero tool-call steps, falling back to loading-lines when empty", async () => {
    const user = userEvent.setup();
    const { now, project, agent, sessionA, sessionB } = makeFixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [sessionA, sessionB],
      runningSessionIds: [sessionA.session_id],
      liveRunBySession: {
        [sessionA.session_id]: {
          session_id: sessionA.session_id,
          run_id: "40000000-0000-4000-8000-000000000901",
          streaming_text: "",
          context_used_tokens: 12_700,
          context_window_tokens: 1_000_000,
          started_at: now - 2_000,
          updated_at: now - 1_000,
        },
      },
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);

    // 1. Empty streaming_text falls back to calm loading-lines skeleton (no live bubble).
    expect(await screen.findByTestId("thinking-indicator")).toBeInTheDocument();
    expect(screen.queryByTestId("live-streaming-bubble")).not.toBeInTheDocument();

    // 2. When streaming_text arrives (including raw HTML to verify skipHtml safety),
    // clean Markdown bubble + quiet working indicator appear with ZERO tool-call noise.
    bridge.liveRunBySession[sessionA.session_id] = {
      session_id: sessionA.session_id,
      run_id: "40000000-0000-4000-8000-000000000901",
      streaming_text:
        "Проверяю **контракты** модулей.\n\n<script>window.__xss=1</script><img src=x onerror=alert(1)>\n\nГотовлю финальный вывод.",
      context_used_tokens: 12_700,
      context_window_tokens: 1_000_000,
      started_at: now - 2_000,
      updated_at: now,
    };

    const liveBubble = await screen.findByTestId("live-streaming-bubble", {}, { timeout: 5_000 });
    expect(liveBubble).toHaveClass("message-row", "assistant", "live-streaming-bubble");
    expect(within(liveBubble).getByText("контракты").tagName).toBe("STRONG");
    expect(liveBubble).toHaveTextContent("Готовлю финальный вывод.");
    expect(within(liveBubble).getByTestId("live-working-indicator")).toHaveTextContent(
      "Продолжает работу…",
    );
    expect(screen.queryByTestId("thinking-indicator")).not.toBeInTheDocument();

    // Security (skipHtml): raw <script> and <img> tags are NEVER mounted into the DOM.
    expect(liveBubble.querySelector("script")).toBeNull();
    expect(liveBubble.querySelector("img")).toBeNull();

    // Critical owner rule: ZERO tool-call feeds, phase badges, or context token counters in chat timeline!
    const timeline = screen.getByTestId("message-timeline");
    expect(timeline.textContent).not.toMatch(/Шаги агента|read_file|12\.7K|1\.0M/i);

    // 3. Switching to another session immediately clears liveRun so nothing leaks across sessions.
    await user.click(screen.getByTestId(`session-item-${sessionB.session_id}`));
    expect(screen.queryByTestId("live-streaming-bubble")).not.toBeInTheDocument();
    expect(screen.queryByTestId("thinking-indicator")).not.toBeInTheDocument();
  });

  it("keeps Composer interactive mid-run: Enter enqueues followup, Ctrl+Enter triggers steerSession, Stop stays available", async () => {
    const user = userEvent.setup();
    const { project, agent, sessionA } = makeFixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [sessionA],
      sendDelayMs: 12_000,
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);

    const input = await screen.findByLabelText("Поле ввода сообщения");
    await user.type(input, "Стартовая задача{enter}");
    await screen.findByTestId("running-control-bar");

    // Mid-run textarea stays enabled with Russian queue/steer placeholder and Stop button visible.
    expect(input).toBeEnabled();
    expect(input).toHaveAttribute(
      "placeholder",
      "Уточнить задачу — Enter в очередь, Ctrl+Enter прервать и уточнить",
    );
    expect(screen.getByTestId("composer-stop-button")).toBeEnabled();

    // Typing mid-run reveals tactile Queue & Steer buttons beside Stop.
    await user.type(input, "Добавь тесты на края");
    expect(screen.getByTestId("composer-queue-followup-btn")).toBeEnabled();
    expect(screen.getByTestId("composer-steer-now-btn")).toBeEnabled();
    expect(screen.getByTestId("composer-stop-button")).toBeEnabled();

    // Plain Enter enqueues follow-up and clears draft.
    await user.keyboard("{Enter}");
    await waitFor(() => {
      expect(bridge.calls.some((c) => c.method === "enqueueFollowup")).toBe(true);
    });
    expect(input).toHaveValue("");
    const queueBar = await screen.findByTestId("followup-queue-bar");
    expect(within(queueBar).getByTestId("followup-queued-status")).toHaveTextContent("В очереди · 1");
    expect(queueBar).toHaveTextContent("Добавь тесты на края");

    // Ctrl+Enter triggers immediate live steering and clears draft without flashing a Stop notice.
    bridge.sendDelayMs = 0;
    await user.type(input, "Смени приоритет на безопасность");
    fireEvent.keyDown(input, { key: "Enter", ctrlKey: true });

    await waitFor(() => {
      const steerCalls = bridge.calls.filter((c) => c.method === "steerSession");
      expect(steerCalls).toHaveLength(1);
      expect(
        (steerCalls[0]?.args as { request: { content: string } }).request.content,
      ).toBe("Смени приоритет на безопасность");
    });
    expect(input).toHaveValue("");
    expect(await screen.findByText(/Принято .*Смени приоритет на безопасность/)).toBeInTheDocument();
    expect(screen.queryByTestId("cancelled-run-notice")).not.toBeInTheDocument();
  });

  it("supports follow-up inline edit (Enter saves, Esc cancels without stopping run), delete, promote-to-steer, pause-on-stop, resume, and clear", async () => {
    const user = userEvent.setup();
    const { now, project, agent, sessionA } = makeFixture();
    const seededFollowups: SessionFollowupRecord[] = [
      {
        followup_id: "98500000-0000-4000-8000-000000000001",
        session_id: sessionA.session_id,
        content: "Первое уточнение",
        status: "queued",
        position: 1,
        idempotency_key: "k-1",
        dispatched_run_id: null,
        created_at: now - 2_000,
        updated_at: now - 2_000,
      },
      {
        followup_id: "98500000-0000-4000-8000-000000000002",
        session_id: sessionA.session_id,
        content: "Второе уточнение",
        status: "queued",
        position: 2,
        idempotency_key: "k-2",
        dispatched_run_id: null,
        created_at: now - 1_000,
        updated_at: now - 1_000,
      },
    ];
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [sessionA],
      runningSessionIds: [sessionA.session_id],
      followupsBySession: {
        [sessionA.session_id]: seededFollowups,
      },
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);

    const queueBar = await screen.findByTestId("followup-queue-bar");
    expect(within(queueBar).getByTestId("followup-queued-status")).toHaveTextContent("В очереди · 2");

    // 1. Inline edit: Esc cancels editing WITHOUT cancelling active run!
    await user.click(screen.getByTestId(`followup-edit-btn-${seededFollowups[0]!.followup_id}`));
    const editInput = await screen.findByTestId(
      `followup-edit-input-${seededFollowups[0]!.followup_id}`,
    );
    await user.clear(editInput);
    await user.type(editInput, "Временный текст");
    await user.keyboard("{Escape}");
    expect(
      screen.queryByTestId(`followup-edit-input-${seededFollowups[0]!.followup_id}`),
    ).not.toBeInTheDocument();
    expect(bridge.calls.filter((c) => c.method === "cancelRun")).toHaveLength(0);
    expect(queueBar).toHaveTextContent("Первое уточнение");

    // 2. Inline edit: Enter saves updated text via updateFollowup.
    await user.click(screen.getByTestId(`followup-edit-btn-${seededFollowups[0]!.followup_id}`));
    const editInput2 = await screen.findByTestId(
      `followup-edit-input-${seededFollowups[0]!.followup_id}`,
    );
    await user.clear(editInput2);
    await user.type(editInput2, "Обновлённое первое уточнение{enter}");
    await waitFor(() => {
      expect(bridge.calls.some((c) => c.method === "updateFollowup")).toBe(true);
    });
    expect(queueBar).toHaveTextContent("Обновлённое первое уточнение");

    // 3. Delete second item via deleteFollowup.
    await user.click(screen.getByTestId(`followup-delete-btn-${seededFollowups[1]!.followup_id}`));
    await waitFor(() => {
      expect(screen.queryByTestId(`followup-item-${seededFollowups[1]!.followup_id}`)).not.toBeInTheDocument();
    });
    expect(within(queueBar).getByTestId("followup-queued-status")).toHaveTextContent("В очереди · 1");

    // 4. Stop active run -> queue switches to calm amber paused state with Продолжить & Очистить.
    await user.click(screen.getByTestId("composer-stop-button"));
    expect(await screen.findByTestId("followup-paused-status")).toHaveTextContent(
      "Очередь на паузе после остановки",
    );
    expect(screen.getByTestId("followup-resume-btn")).toBeEnabled();
    expect(screen.getByTestId("followup-clear-btn")).toBeEnabled();

    // 5. Click Продолжить -> calls resumeFollowups and dispatches the paused item cleanly.
    await user.click(screen.getByTestId("followup-resume-btn"));
    await waitFor(() => {
      expect(bridge.calls.some((c) => c.method === "resumeFollowups")).toBe(true);
    });
    expect(
      await screen.findByText(/Принято .*Обновлённое первое уточнение/),
    ).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.queryByTestId("followup-queue-bar")).not.toBeInTheDocument();
    });

    // 6. Test Promote-to-Steer (deletes followup and immediately calls steerSession)
    // and Clear (Очистить) while a run is active.
    bridge.sendDelayMs = 5_000;
    const third = await bridge.enqueueFollowup(sessionA.session_id, {
      content: "Продвинуть прямо сейчас",
    });
    await bridge.enqueueFollowup(sessionA.session_id, {
      content: "Удалить очисткой",
    });
    await user.click(screen.getByTestId(`session-item-${sessionA.session_id}`));
    await screen.findByTestId(`followup-item-${third.followup_id}`);

    await user.click(screen.getByTestId(`followup-promote-btn-${third.followup_id}`));
    await waitFor(() => {
      const steerCalls = bridge.calls.filter((c) => c.method === "steerSession");
      expect(steerCalls).toHaveLength(1);
      expect(
        (steerCalls[0]?.args as { request: { content: string } }).request.content,
      ).toBe("Продвинуть прямо сейчас");
    });

    // Clear remaining queued followup via Очистить while the steered turn is running.
    const clearBtn = await screen.findByTestId("followup-clear-btn");
    await user.click(clearBtn);
    await waitFor(() => {
      expect(bridge.calls.some((c) => c.method === "clearFollowups")).toBe(true);
      expect(screen.queryByTestId("followup-queue-bar")).not.toBeInTheDocument();
    });

    // Cleanly stop the slow steered turn before test teardown.
    await user.click(screen.getByTestId("composer-stop-button"));
    await waitFor(() => {
      expect(screen.queryByTestId("running-control-bar")).not.toBeInTheDocument();
    });
  });

  it("displays context_used_tokens / context_window_tokens in StatsPopover for both live and persisted runs, never inside chat timeline", async () => {
    const user = userEvent.setup();
    const { now, project, agent, sessionA, sessionB } = makeFixture();
    const persistedRunB: RunRecord = {
      run_id: "40000000-0000-4000-8000-000000000202",
      session_id: sessionB.session_id,
      agent_id: agent.agent_id,
      agent_config_version_id: agent.current_config_version_id,
      user_message_id: "50000000-0000-4000-8000-000000000202",
      assistant_message_id: "51000000-0000-4000-8000-000000000202",
      status: "succeeded",
      rehydrated: false,
      context_used_tokens: 4_096,
      context_window_tokens: 1_048_576,
      correlation_id: "corr-b",
      created_at: now - 4_000,
      finished_at: now - 3_000,
    };
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [sessionA, sessionB],
      runningSessionIds: [sessionA.session_id],
      liveRunBySession: {
        [sessionA.session_id]: {
          session_id: sessionA.session_id,
          run_id: "40000000-0000-4000-8000-000000000201",
          streaming_text: "Идёт анализ…",
          context_used_tokens: 12_700,
          context_window_tokens: 1_000_000,
          started_at: now - 2_000,
          updated_at: now,
        },
      },
      runsBySession: {
        [sessionB.session_id]: [persistedRunB],
      },
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);

    // Wait for live snapshot to populate on sessionA
    await screen.findByTestId("live-streaming-bubble", {}, { timeout: 5_000 });
    expect(screen.getByTestId("message-timeline").textContent).not.toContain("12.7K");

    // Open StatsPopover -> shows live run context usage `12.7K / 1.0M`
    await user.click(screen.getByTestId("system-status-pill"));
    const statsPop = await screen.findByTestId("stats-popover");
    expect(within(statsPop).getByTestId("stats-context-usage")).toHaveTextContent("12.7K / 1.0M");

    // Close StatsPopover, switch to idle sessionB, open StatsPopover -> shows persisted run `4.1K / 1.0M`
    await user.keyboard("{Escape}");
    await user.click(screen.getByTestId(`session-item-${sessionB.session_id}`));
    await user.click(screen.getByTestId("system-status-pill"));
    const statsPopB = await screen.findByTestId("stats-popover");
    expect(within(statsPopB).getByTestId("stats-context-usage")).toHaveTextContent("4.1K / 1.0M");
  });

  it("routes all 7 D3+ endpoints cleanly in devHttpBridge including 204 No Content deletes", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const method = init?.method ?? "GET";
      if (method === "DELETE") {
        return new Response(null, { status: 204 });
      }
      if (url.endsWith("/live")) {
        return new Response(JSON.stringify({ active: null, followups: [] }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        });
      }
      if (url.endsWith("/followups/resume")) {
        return new Response(JSON.stringify({ followups: [], dispatched: null }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        });
      }
      return new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    });
    vi.stubGlobal("fetch", fetchMock);

    const httpBridge = createDevHttpBridge("/__dennett_dev_head");
    const sid = "sess-http-1";
    const fid = "fu-http-1";

    await httpBridge.getSessionLive(sid);
    await httpBridge.enqueueFollowup(sid, { content: "q1", idempotency_key: "ik1" });
    await httpBridge.updateFollowup(sid, fid, { content: "q1-edited" });
    await httpBridge.deleteFollowup(sid, fid);
    await httpBridge.clearFollowups(sid);
    await httpBridge.resumeFollowups(sid);
    await httpBridge.steerSession(sid, { content: "steer-now", client_message_id: "cm-1" });

    expect(fetchMock.mock.calls.map(([u, init]) => `${init?.method ?? "GET"} ${String(u)}`)).toEqual([
      `GET /__dennett_dev_head/v1/sessions/${sid}/live`,
      `POST /__dennett_dev_head/v1/sessions/${sid}/followups`,
      `PATCH /__dennett_dev_head/v1/sessions/${sid}/followups/${fid}`,
      `DELETE /__dennett_dev_head/v1/sessions/${sid}/followups/${fid}`,
      `DELETE /__dennett_dev_head/v1/sessions/${sid}/followups`,
      `POST /__dennett_dev_head/v1/sessions/${sid}/followups/resume`,
      `POST /__dennett_dev_head/v1/sessions/${sid}/steer`,
    ]);
  });
});
