import React from "react";
import axe from "axe-core";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "./App";

describe("Project Chat workbench", () => {
  it("renders the approved workbench zones and truthful runtime state", async () => {
    render(<App />);

    expect(screen.getByRole("navigation", { name: "Primary navigation" })).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Project navigation" })).toBeInTheDocument();
    expect(screen.getByRole("main")).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Read-only context inspector" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Message composer" })).toBeInTheDocument();
    expect(await screen.findByText("Codex is checking the renderer. You can steer or stop this session.")).toBeVisible();

    const messages = screen.getAllByRole("article");
    expect(messages.some((message) => message.classList.contains("message--user"))).toBe(true);
    expect(messages.some((message) => message.classList.contains("message--agent"))).toBe(true);
    expect(screen.getByRole("tab", { name: "Changes" })).toBeDisabled();
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
    expect(await screen.findByRole("heading", { name: "Start with the project itself" })).toBeVisible();
  });

  it("scopes Stop to the active session and preserves partial output", async () => {
    const user = userEvent.setup();
    render(<App />);
    const stop = await screen.findByRole("button", {
      name: 'Stop generation for session "First Project Chat screen"',
    });

    await user.click(stop);
    expect(await screen.findByText("Generation stopped for this session. The partial response is preserved.")).toBeVisible();
    expect(screen.queryByRole("button", { name: /Stop generation for session/ })).not.toBeInTheDocument();
  });

  it("supports keyboard composition and bounded live announcements", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("Codex is checking the renderer. You can steer or stop this session.");
    const composer = screen.getByRole("textbox", { name: "Message to project agent" });

    await user.click(composer);
    await user.type(composer, "Review the owner checkpoint");
    await act(async () => {
      fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    });
    expect(screen.getByRole("dialog", { name: "Command Center" })).toBeVisible();
    expect(screen.getByRole("textbox", { name: "Command Center search" })).toHaveFocus();
    await act(async () => {
      fireEvent.keyDown(window, { key: "Escape" });
    });
    expect(screen.queryByRole("dialog", { name: "Command Center" })).not.toBeInTheDocument();

    await user.click(composer);
    await act(async () => {
      fireEvent.keyDown(composer, { key: "Enter", ctrlKey: true });
    });
    expect(await screen.findByText("Review the owner checkpoint")).toBeVisible();
    expect(screen.getByText("Draft added to this local preview. No runtime command was sent.")).toBeInTheDocument();
  });

  it("has no automated accessibility violations in the default state", async () => {
    const { container } = render(<App />);
    await screen.findByText("Codex is checking the renderer. You can steer or stop this session.");

    const result = await axe.run(container, {
      rules: {
        "color-contrast": { enabled: false },
      },
    });
    expect(result.violations, result.violations.map((violation) => violation.help).join("\n")).toEqual([]);
  });

  it("keeps focus stable when a fixture update arrives", async () => {
    const user = userEvent.setup();
    render(<App />);
    const composer = screen.getByRole("textbox", { name: "Message to project agent" });
    await user.click(composer);
    expect(composer).toHaveFocus();

    const selector = screen.getByRole("combobox", { name: "Preview state" });
    selector.focus();
    await user.selectOptions(selector, "resyncing");
    await waitFor(() => expect(screen.getByText("Refreshing the session snapshot after a revision gap.")).toBeVisible());
    expect(selector).toHaveFocus();
  });
});
