import React from "react";
import axe from "axe-core";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "./App";
import { createFixtureDennettClient, type DennettClient } from "./fixtures/projectChat";

describe("Project Chat workbench", () => {
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

    const messages = screen.getAllByRole("article");
    expect(messages.some((message) => message.classList.contains("message--user"))).toBe(true);
    expect(messages.some((message) => message.classList.contains("message--agent"))).toBe(true);
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

    expect(within(selector).getAllByRole("option")).toHaveLength(9);
    await user.selectOptions(selector, "stale");
    expect(await screen.findByText("This view is behind the authoritative revision. Mutating actions are unavailable.")).toBeVisible();
    expect(screen.getByText("Last synced 11 min ago")).toBeVisible();

    await user.selectOptions(selector, "empty");
    expect(await screen.findByRole("heading", { name: "Start with the project" })).toBeVisible();
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

  it("provides working access and runtime presentation controls", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("Codex is checking the renderer. You can steer or stop this session.");

    await user.click(screen.getByRole("button", { name: "Full access" }));
    const accessDialog = screen.getByRole("dialog", { name: "Agent access" });
    await user.click(within(accessDialog).getByRole("button", { name: "Auto-approve" }));
    expect(screen.getByRole("button", { name: "Auto-approve" })).toBeVisible();

    await user.click(screen.getByRole("button", { name: /CodexHigh/ }));
    const runtimeDialog = screen.getByRole("dialog", { name: "Agent runtime" });
    expect(within(runtimeDialog).getByText("Codex SDK")).toBeVisible();
    await user.click(within(runtimeDialog).getByRole("button", { name: "Medium" }));
    expect(screen.getByRole("button", { name: /CodexMedium/ })).toBeVisible();
    await user.click(screen.getByRole("textbox", { name: "Message to project agent" }));
    expect(screen.queryByRole("dialog", { name: "Agent runtime" })).not.toBeInTheDocument();

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

  it("has no automated accessibility violations in the default state", async () => {
    const { container } = render(<App />);
    await screen.findByText("Codex is checking the renderer. You can steer or stop this session.");

    const result = await axe.run(container, { rules: { "color-contrast": { enabled: false } } });
    expect(result.violations, result.violations.map((violation) => violation.help).join("\n")).toEqual([]);
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
