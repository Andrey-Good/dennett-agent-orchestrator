import assert from "node:assert/strict";
import { test } from "node:test";

import {
  codexAccessConfiguration,
  codexAccessThreadOptions,
  codexRuntimeConfiguration,
} from "./codex-model-catalog.js";
import { RuntimeAdapterError } from "./runtime-contract.js";

const catalog = {
  models: [
    {
      slug: "gpt-new",
      display_name: "GPT New",
      description: "Primary model",
      visibility: "list",
      priority: 1,
      default_reasoning_level: "high",
      supported_reasoning_levels: [
        { effort: "low", description: "Fast reasoning" },
        { effort: "high", description: "Deep reasoning" },
        { effort: "ultra", description: "Longest reasoning" },
      ],
      service_tiers: [
        { id: "fast", name: "Fast", description: "Priority processing" },
      ],
    },
    {
      slug: "gpt-small",
      display_name: "GPT Small",
      visibility: "list",
      priority: 2,
      default_reasoning_level: "low",
      supported_reasoning_levels: [{ effort: "low" }],
      service_tiers: [],
    },
    {
      slug: "hidden-model",
      display_name: "Hidden",
      visibility: "hide",
      priority: 0,
      default_reasoning_level: "low",
      supported_reasoning_levels: [{ effort: "low" }],
      service_tiers: [],
    },
  ],
};

test("Codex catalog publishes only real provider choices and their dependencies", () => {
  const configuration = codexRuntimeConfiguration(catalog);
  assert.deepEqual(configuration.controls.map((control) => control.label), [
    "Agent access",
    "Model",
    "Reasoning",
    "Speed",
  ]);
  const model = configuration.controls.find((control) => control.id === "model");
  const reasoning = configuration.controls.find((control) => control.id === "reasoning_effort");
  const speed = configuration.controls.find((control) => control.id === "service_tier");
  assert.deepEqual(
    model?.choices.map((choice) => choice.id),
    ["gpt-new", "gpt-small"],
  );
  assert.deepEqual(
    reasoning?.choices.find((choice) => choice.id === "ultra")?.availableWhen,
    [{ controlId: "model", choiceIds: ["gpt-new"] }],
  );
  assert.equal(reasoning?.defaultChoiceId, "provider_default");
  assert.equal(reasoning?.choices[0]?.label, "Model default");
  assert.deepEqual(
    speed?.choices.find((choice) => choice.id === "fast")?.availableWhen,
    [{ controlId: "model", choiceIds: ["gpt-new"] }],
  );
});

test("Codex catalog resolves per-turn model, reasoning and speed selections", () => {
  const configuration = codexRuntimeConfiguration(catalog);
  assert.deepEqual(configuration.resolve([
    { controlId: "dennett.access_mode", choiceId: "full_access" },
    { controlId: "model", choiceId: "gpt-new" },
    { controlId: "reasoning_effort", choiceId: "ultra" },
    { controlId: "service_tier", choiceId: "fast" },
  ]), {
    accessMode: "full_access",
    model: "gpt-new",
    reasoningEffort: "ultra",
    serviceTier: "fast",
  });
  assert.deepEqual(configuration.resolve([
    { controlId: "model", choiceId: "gpt-small" },
    { controlId: "reasoning_effort", choiceId: "provider_default" },
  ]), {
    accessMode: "auto_approve",
    model: "gpt-small",
    reasoningEffort: "low",
    serviceTier: null,
  });
});

test("Codex access choices map to enforced sandbox modes without manual approvals", () => {
  const access = codexAccessConfiguration();
  assert.deepEqual(access.controls[0]?.choices.map((choice) => choice.label), [
    "Auto-approve",
    "Full access",
  ]);
  assert.deepEqual(codexAccessThreadOptions(access.resolve([])), {
    approvalPolicy: "never",
    networkAccessEnabled: true,
    sandboxMode: "workspace-write",
    webSearchMode: "live",
  });
  assert.deepEqual(codexAccessThreadOptions(access.resolve([
    { controlId: "dennett.access_mode", choiceId: "full_access" },
  ])), {
    approvalPolicy: "never",
    networkAccessEnabled: true,
    sandboxMode: "danger-full-access",
    webSearchMode: "live",
  });
});

test("Codex catalog rejects unavailable and unknown combinations", () => {
  const configuration = codexRuntimeConfiguration(catalog);
  for (const selections of [
    [
      { controlId: "model", choiceId: "gpt-small" },
      { controlId: "reasoning_effort", choiceId: "ultra" },
    ],
    [
      { controlId: "model", choiceId: "gpt-small" },
      { controlId: "service_tier", choiceId: "fast" },
    ],
    [{ controlId: "unknown", choiceId: "value" }],
  ]) {
    assert.throws(
      () => configuration.resolve(selections),
      (error: unknown) => error instanceof RuntimeAdapterError && error.code === "invalid_request",
    );
  }
});
