import React from "react";
import axe from "axe-core";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "./App";
import { createFixtureDennettClient, type DennettClient } from "./fixtures/projectChat";
import stylesCss from "./styles.css?raw";
import tauriConfigRaw from "../src-tauri/tauri.conf.json?raw";

const fixtureExpectations = [
  ["streaming", "Codex is checking the renderer. You can steer or stop this session."],
  ["restored", "The session was restored from the authoritative local snapshot."],
  ["cached", "Showing the last local snapshot while the node reconnects."],
  ["stopped", "Generation stopped for this session. The partial response is preserved."],
  ["timed-out", "The runtime did not acknowledge completion. Retry when the connection is healthy."],
  ["stale", "This view is behind the authoritative revision. Mutating actions are unavailable."],
  ["resyncing", "Refreshing the session snapshot after a revision gap."],
  ["loading", "Opening the local Project Chat snapshot."],
  ["empty", "Start a direct conversation with the project agent."],
] as const;

function colorToken(name: string): string {
  const match = stylesCss.match(new RegExp(`--${name}:\\s*(#[0-9a-f]{6})`, "i"));
  if (!match) throw new Error(`Missing solid color token --${name}`);
  return match[1];
}

function relativeLuminance(hex: string): number {
  const channels = hex.slice(1).match(/../g)?.map((value) => Number.parseInt(value, 16) / 255) ?? [];
  const [red, green, blue] = channels.map((value) => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

function contrastRatio(foreground: string, background: string): number {
  const foregroundLuminance = relativeLuminance(foreground);
  const backgroundLuminance = relativeLuminance(background);
  return (Math.max(foregroundLuminance, backgroundLuminance) + 0.05)
    / (Math.min(foregroundLuminance, backgroundLuminance) + 0.05);
}

describe("Project Chat workbench", () => {
  it("uses native Mica without projecting wallpaper inside React", () => {
    const tauriConfig = JSON.parse(tauriConfigRaw);
    const mainWindow = tauriConfig.app.windows.find((windowConfig: { label?: string }) => windowConfig.label === "main");

    expect(mainWindow).toMatchObject({
      transparent: true,
      windowEffects: { effects: ["mica"] },
    });
    expect(mainWindow).not.toHaveProperty("backgroundColor");

    Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });
    const view = render(<App />);
    expect(document.documentElement).toHaveClass("native-shell");
    expect(view.container.querySelector(".desktop-wallpaper")).not.toBeInTheDocument();

    view.unmount();
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  });

  it("renders the owner-directed zones and truthful runtime state", async () => {
    render(<App />);

    expect(screen.getByRole("navigation", { name: "Primary navigation" })).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Project and chat navigation" })).toBeInTheDocument();
    expect(screen.getByRole("main")).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Workspace resources" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Message composer" })).toBeInTheDocument();
    expect(await screen.findByText("Codex is checking the renderer. You can steer or stop this session.")).toBeVisible();

    const location = screen.getByRole("navigation", { name: "Current location" });
    expect(location).toHaveTextContent("Projects/dennett-agent-orchestrator/Project Chat owner checkpoint");
    expect(screen.queryByText("Local node")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Scope: Project")).not.toBeInTheDocument();
    const primaryNavigation = screen.getByRole("navigation", { name: "Primary navigation" });
    expect(within(primaryNavigation).getByRole("button", { name: "Chats" })).toBeVisible();
    expect(within(primaryNavigation).queryByRole("button", { name: "New chat" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Install available update" })).not.toBeInTheDocument();

    const messages = screen.getAllByRole("article");
    expect(messages.some((message) => message.classList.contains("message--user"))).toBe(true);
    expect(messages.some((message) => message.classList.contains("message--agent"))).toBe(true);
  });

  it("places project and chat creation controls beside the collection they affect", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("Codex is checking the renderer. You can steer or stop this session.");

    const sidebar = screen.getByRole("complementary", { name: "Project and chat navigation" });
    const newProject = within(sidebar).getByRole("button", { name: "New project" });
    newProject.focus();
    await user.keyboard("[Enter]");
    const projectDialog = screen.getByRole("dialog", { name: "Create or add project" });
    expect(within(projectDialog).getByRole("button", { name: /Create empty project/ })).toHaveFocus();
    expect(within(projectDialog).getByRole("button", { name: /Add existing folder/ })).toBeVisible();

    await user.keyboard("[Escape]");
    expect(screen.queryByRole("dialog", { name: "Create or add project" })).not.toBeInTheDocument();
    await waitFor(() => expect(newProject).toHaveFocus());
    await user.keyboard("[Enter]");

    const reopenedProjectDialog = screen.getByRole("dialog", { name: "Create or add project" });
    await user.click(within(reopenedProjectDialog).getByRole("button", { name: /Create empty project/ }));
    await waitFor(() => expect(newProject).toHaveFocus());
    expect(within(sidebar).getByText("Untitled project 1")).toBeVisible();
    await user.hover(within(sidebar).getByText("Untitled project 1"));
    await user.click(within(sidebar).getByRole("button", { name: "New chat in Untitled project 1" }));
    expect(screen.getByRole("navigation", { name: "Current location" })).toHaveTextContent("Projects/Untitled project 1/Untitled chat");

    await user.hover(within(sidebar).getByRole("heading", { name: "Recent" }));
    await user.click(within(sidebar).getByRole("button", { name: "New recent chat" }));
    expect(screen.getByRole("navigation", { name: "Current location" })).toHaveTextContent("Chats/Untitled chat");
  });

  it("groups project chats before standalone recent chats", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("Codex is checking the renderer. You can steer or stop this session.");

    const sidebar = screen.getByRole("complementary", { name: "Project and chat navigation" });
    expect(within(sidebar).getByText("dennett-agent-orchestrator")).toBeVisible();
    expect(within(sidebar).getByRole("heading", { name: "Recent" })).toBeVisible();

    await user.click(within(sidebar).getByRole("button", { name: /Provider adapter notes/ }));
    expect(screen.getByRole("navigation", { name: "Current location" })).toHaveTextContent("Chats/Provider adapter notes");
  });

  it("exposes every material fixture and changes state deterministically", async () => {
    const user = userEvent.setup();
    render(<App />);
    const selector = screen.getByRole("combobox", { name: "Preview state" });

    expect(within(selector).getAllByRole("option")).toHaveLength(fixtureExpectations.length);
    for (const [fixtureId, expectedNotice] of fixtureExpectations) {
      await user.selectOptions(selector, fixtureId);
      expect(await screen.findByText(expectedNotice)).toBeVisible();
    }

    expect(screen.getByText("No messages yet")).toBeVisible();
    expect(await screen.findByRole("heading", { name: "Start with the project" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Задай мне вопросы о моей идее" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Изучи этот репозиторий" })).toBeVisible();

    await user.selectOptions(selector, "loading");
    expect(await screen.findByRole("status", { name: "Loading conversation content" })).toBeVisible();
    expect(screen.queryByRole("heading", { name: "Start with the project" })).not.toBeInTheDocument();
  });

  it("keeps local draft messages scoped to their session and creates a distinct chat", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("Codex is checking the renderer. You can steer or stop this session.");

    const composer = screen.getByRole("textbox", { name: "Message to project agent" });
    await user.type(composer, "Only visible in the owner checkpoint");
    fireEvent.keyDown(composer, { key: "Enter", ctrlKey: true });
    expect(await screen.findByText("Only visible in the owner checkpoint")).toBeVisible();

    const sidebar = screen.getByRole("complementary", { name: "Project and chat navigation" });
    await user.click(within(sidebar).getByRole("button", { name: /M01 protocol epoch/ }));
    expect(screen.queryByText("Only visible in the owner checkpoint")).not.toBeInTheDocument();

    await user.click(within(sidebar).getByRole("button", { name: /Project Chat owner checkpoint/ }));
    expect(await screen.findByText("Only visible in the owner checkpoint")).toBeVisible();

    await user.hover(within(sidebar).getByRole("heading", { name: "Recent" }));
    await user.click(within(sidebar).getByRole("button", { name: "New recent chat" }));
    expect(screen.getByRole("navigation", { name: "Current location" })).toHaveTextContent("Chats/Untitled chat");
    expect(await screen.findByRole("heading", { name: "Start with the project" })).toBeVisible();
    expect(screen.queryByText("Only visible in the owner checkpoint")).not.toBeInTheDocument();
    expect(within(sidebar).getByRole("button", { name: /Untitled chat/ })).toBeVisible();
  });

  it("scopes Stop to the selected session and preserves partial output", async () => {
    const user = userEvent.setup();
    render(<App />);
    const stop = await screen.findByRole("button", { name: 'Stop generation for session "Project Chat owner checkpoint"' });

    await user.click(stop);
    expect(await screen.findByText("Generation stopped for this session. The partial response is preserved.")).toBeVisible();
    expect(screen.queryByRole("button", { name: /Stop generation for session/ })).not.toBeInTheDocument();
  });

  it("supports keyboard composition and a focus-contained Command Center", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("Codex is checking the renderer. You can steer or stop this session.");
    const composer = screen.getByRole("textbox", { name: "Message to project agent" });

    await user.click(composer);
    await user.type(composer, "Review the second owner checkpoint");
    await act(async () => { fireEvent.keyDown(window, { key: "k", ctrlKey: true }); });

    expect(screen.getByRole("dialog", { name: "Command Center" })).toBeVisible();
    const commandSearch = screen.getByRole("textbox", { name: "Command Center search" });
    const lastCommand = screen.getByRole("button", { name: "Open local preview" });
    expect(commandSearch).toHaveFocus();
    fireEvent.keyDown(commandSearch, { key: "Tab", shiftKey: true });
    expect(lastCommand).toHaveFocus();
    fireEvent.keyDown(lastCommand, { key: "Tab" });
    expect(commandSearch).toHaveFocus();
    await act(async () => { fireEvent.keyDown(window, { key: "Escape" }); });
    expect(screen.queryByRole("dialog", { name: "Command Center" })).not.toBeInTheDocument();
    expect(composer).toHaveFocus();

    await act(async () => { fireEvent.keyDown(composer, { key: "Enter", ctrlKey: true }); });
    expect(await screen.findByText("Review the second owner checkpoint")).toBeVisible();
    expect(screen.getByText("Draft added to this local preview. No runtime command was sent.")).toBeInTheDocument();
  });

  it("creates a command-center chat in the currently selected scope", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("Codex is checking the renderer. You can steer or stop this session.");

    await act(async () => { fireEvent.keyDown(window, { key: "k", ctrlKey: true }); });
    await user.click(screen.getByRole("button", { name: "New chat in current project" }));
    expect(screen.getByRole("navigation", { name: "Current location" })).toHaveTextContent("Projects/dennett-agent-orchestrator/Untitled chat");

    const sidebar = screen.getByRole("complementary", { name: "Project and chat navigation" });
    await user.click(within(sidebar).getByRole("button", { name: /Provider adapter notes/ }));
    await act(async () => { fireEvent.keyDown(window, { key: "k", ctrlKey: true }); });
    await user.click(screen.getByRole("button", { name: "New standalone chat" }));
    expect(screen.getByRole("navigation", { name: "Current location" })).toHaveTextContent("Chats/Untitled chat");
  });

  it("provides working access and runtime presentation controls", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("Codex is checking the renderer. You can steer or stop this session.");

    const accessTrigger = screen.getByRole("button", { name: "Full access" });
    expect(accessTrigger).toHaveAttribute("aria-controls", "composer-access-popover");
    await user.click(accessTrigger);
    const accessDialog = screen.getByRole("dialog", { name: "Agent access" });
    expect(within(accessDialog).getByRole("button", { name: "Full access" })).toHaveFocus();
    await user.click(within(accessDialog).getByRole("button", { name: "Auto-approve" }));
    expect(screen.getByRole("button", { name: "Auto-approve" })).toBeVisible();
    expect(accessTrigger).toHaveFocus();

    const runtimeTrigger = screen.getByRole("button", { name: /CodexHigh/ });
    expect(runtimeTrigger).toHaveAttribute("aria-controls", "composer-runtime-popover");
    await user.click(runtimeTrigger);
    const runtimeDialog = screen.getByRole("dialog", { name: "Agent runtime" });
    expect(within(runtimeDialog).getByText("Codex SDK")).toBeVisible();
    expect(within(runtimeDialog).getByRole("button", { name: "Medium" })).toHaveFocus();
    await user.click(within(runtimeDialog).getByRole("button", { name: "Medium" }));
    expect(screen.getByRole("button", { name: /CodexMedium/ })).toBeVisible();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: "Agent runtime" })).not.toBeInTheDocument();
    await waitFor(() => expect(runtimeTrigger).toHaveFocus());

    await user.click(screen.getByRole("button", { name: "Add context" }));
    expect(screen.getByRole("dialog", { name: "Add context" })).toBeVisible();
    expect(screen.getByRole("button", { name: /Files or folders/ })).toBeDisabled();
  });

  it("expands the plan and opens resources in the central workspace", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("Codex is checking the renderer. You can steer or stop this session.");

    await user.click(screen.getByRole("button", { name: /Current plan step/ }));
    expect(screen.getByText("Run visual and interaction QA")).toBeVisible();

    await user.click(screen.getByRole("button", { name: /DennettLocal preview/ }));
    expect(screen.getByRole("region", { name: "Dennett viewer" })).toBeVisible();
    expect(screen.getByTitle("Dennett local preview")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Close viewer and return to chat" }));
    expect(screen.getByRole("region", { name: "Conversation" })).toBeVisible();
  });

  it("has no automated structural accessibility violations in the default state", async () => {
    const { container } = render(<App />);
    await screen.findByText("Codex is checking the renderer. You can steer or stop this session.");

    const result = await axe.run(container, { rules: { "color-contrast": { enabled: false } } });
    expect(result.violations, result.violations.map((violation) => violation.help).join("\n")).toEqual([]);
  });

  it("keeps muted text tokens at WCAG AA contrast on the lightest dark surface", () => {
    const lightestSurface = colorToken("surface-active");
    const foregrounds = [colorToken("text"), colorToken("text-muted"), colorToken("text-faint")];

    for (const foreground of foregrounds) {
      expect(contrastRatio(foreground, lightestSurface), `${foreground} on ${lightestSurface}`).toBeGreaterThanOrEqual(4.5);
    }
    expect(contrastRatio(colorToken("text-faint"), colorToken("message-user"))).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio(colorToken("text-inverse"), colorToken("surface-inverse"))).toBeGreaterThanOrEqual(4.5);
  });

  it("routes every solid text foreground through a contrast-tested semantic token", () => {
    const literalTextColors = [...stylesCss.matchAll(/(?:^|[;{])\s*color:\s*#[0-9a-f]{6}/gim)].map((match) => match[0].trim());

    expect(literalTextColors).toEqual([]);
  });

  it("keeps focus stable when a fixture update arrives", async () => {
    const user = userEvent.setup();
    render(<App />);
    const selector = screen.getByRole("combobox", { name: "Preview state" });
    selector.focus();
    await user.selectOptions(selector, "resyncing");
    await waitFor(() => expect(screen.getByText("Refreshing the session snapshot after a revision gap.")).toBeVisible());
    expect(selector).toHaveFocus();
  });

  it("keeps deferred affordances disabled and collapse controls truthful", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("Codex is checking the renderer. You can steer or stop this session.");

    expect(screen.getByRole("button", { name: "Minimize — available in desktop shell" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Tasks — available in a later milestone" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Voice mode — available in a later milestone" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Collapse workspace resources" }));
    expect(screen.getByRole("navigation", { name: "Collapsed workspace resources" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Show workspace resources" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Show workspace resources" }));
    expect(screen.getByRole("heading", { name: "Workspace" })).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Hide project navigation" }));
    expect(screen.queryByRole("complementary", { name: "Project and chat navigation" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Show project navigation" }));
    expect(screen.getByRole("complementary", { name: "Project and chat navigation" })).toBeVisible();
  });

  it("keeps fixture selection outside the transport-neutral client request", async () => {
    const client: DennettClient = createFixtureDennettClient("cached");
    const snapshot = await client.readProjectChat({ projectId: "dennett-agent-orchestrator", sessionId: "session-1" });

    expect(snapshot.state).toBe("cached");
    expect(snapshot.notice).toContain("local snapshot");
  });
});
