import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";

import { CodexCanaryError } from "./codex-canary-lib.js";
import {
  assertNoApiKeyEnvironment,
  createSubscriptionCodexOptions,
  inspectCodexCli,
  prepareRuntimeCodexHome,
  resolveCanaryCodexHomeDirectory,
  subscriptionCliArguments,
  type ProcessRunner,
} from "./codex-cli.js";

test("API key and injected access-token modes are rejected before canary execution", () => {
  for (const name of ["OPENAI_API_KEY", "CODEX_API_KEY", "CODEX_ACCESS_TOKEN"]) {
    assert.throws(
      () => assertNoApiKeyEnvironment({ [name.toLowerCase()]: "private-secret" }),
      (error: unknown) =>
        error instanceof CodexCanaryError &&
        error.code === "api_key_environment_present",
    );
  }

  assert.doesNotThrow(() => assertNoApiKeyEnvironment({}));
});

test("subscription options pin ChatGPT and remove custom-provider environment", () => {
  const options = createSubscriptionCodexOptions(
    {
      USERPROFILE: "C:/Users/synthetic",
      Path: "C:/Windows/System32",
      CODEX_HOME: "C:/private/custom-codex-home",
      OPENAI_API_KEY: "private-openai-key",
      OPENAI_BASE_URL: "https://private-proxy.example",
      CUSTOM_PROVIDER_TOKEN: "private-provider-token",
      MISTRAL_API_KEY: "private-mistral-key",
    },
    {
      codexHomeDirectory: "C:/synthetic/codex-home",
      gitConfigFile: "C:/synthetic/isolation/gitconfig",
      osHomeDirectory: "C:/synthetic/isolation/home",
      workingDirectory: "C:/synthetic/workspace",
    },
  );

  assert.deepEqual(options.env, {
    PATH: "C:/Windows/System32",
    HOME: "C:/synthetic/isolation/home",
    USERPROFILE: "C:/synthetic/isolation/home",
    CODEX_HOME: "C:/synthetic/codex-home",
    GIT_CEILING_DIRECTORIES: "C:/synthetic/workspace",
    GIT_CONFIG_COUNT: "0",
    GIT_CONFIG_GLOBAL: "C:/synthetic/isolation/gitconfig",
    GIT_CONFIG_NOSYSTEM: "1",
  });
  assert.equal(options.apiKey, undefined);
  assert.equal(options.baseUrl, undefined);
  assert.equal(options.codexPathOverride, undefined);
  assert.equal(options.config?.model_provider, "openai");
  assert.equal(options.config?.forced_login_method, "chatgpt");
  assert.equal(
    options.config?.chatgpt_base_url,
    "https://chatgpt.com/backend-api/",
  );
  assert.equal(options.config?.cli_auth_credentials_store, "file");
  assert.deepEqual(options.config?.history, { persistence: "none" });
});

test("canary auth uses a dedicated Dennett state directory", () => {
  assert.equal(
    resolveCanaryCodexHomeDirectory({
      LOCALAPPDATA: "C:/Users/synthetic/AppData/Local",
      USERPROFILE: "C:/Users/synthetic",
    }),
    path.join(
      "C:/Users/synthetic/AppData/Local",
      "Dennett",
      "codex-canary-auth",
    ),
  );
});

test("CLI inspection invokes the official launcher and accepts only ChatGPT login", async () => {
  const calls: string[][] = [];
  const chatGptRunner: ProcessRunner = async (_executable, args) => {
    calls.push(args);
    return {
      exitCode: 0,
      stdout: args.at(-1) === "--version" ? "codex-cli 0.144.5\n" : "",
      stderr:
        args.at(-1) === "--version" ? "" : "Logged in using ChatGPT\n",
    };
  };
  assert.deepEqual(
    await inspectCodexCli("C:/synthetic/codex.js", {}, chatGptRunner),
    {
      authMode: "chatgpt",
      cliVersion: "codex-cli 0.144.5",
    },
  );
  assert.deepEqual(calls, [
    ["C:/synthetic/codex.js", "--version"],
    [
      "C:/synthetic/codex.js",
      ...subscriptionCliArguments(["login", "status"]),
    ],
  ]);

  const apiKeyRunner: ProcessRunner = async (_executable, args) => ({
    exitCode: 0,
    stdout: args.at(-1) === "--version" ? "codex-cli 0.144.5\n" : "",
    stderr:
      args.at(-1) === "--version" ? "" : "Logged in using an API key\n",
  });
  await assert.rejects(
    inspectCodexCli("C:/synthetic/codex.js", {}, apiKeyRunner),
    (error: unknown) =>
      error instanceof CodexCanaryError &&
      error.code === "chatgpt_login_required",
  );
});

test("CLI inspection rejects ambiguous login text and version drift", async () => {
  for (const ambiguousStatus of [
    "Not Logged in using ChatGPT",
    "Logged in using ChatGPT and using an API key",
    "warning\nLogged in using ChatGPT",
  ]) {
    const runner: ProcessRunner = async (_executable, args) => ({
      exitCode: 0,
      stdout: args.at(-1) === "--version" ? "codex-cli 0.144.5\n" : "",
      stderr: args.at(-1) === "--version" ? "" : `${ambiguousStatus}\n`,
    });
    await assert.rejects(
      inspectCodexCli("C:/synthetic/codex.js", {}, runner),
      (error: unknown) =>
        error instanceof CodexCanaryError &&
        error.code === "chatgpt_login_required",
    );
  }

  const driftedVersionRunner: ProcessRunner = async (_executable, args) => ({
    exitCode: 0,
    stdout: args.at(-1) === "--version" ? "codex-cli 0.144.4\n" : "",
    stderr:
      args.at(-1) === "--version" ? "" : "Logged in using ChatGPT\n",
  });
  await assert.rejects(
    inspectCodexCli("C:/synthetic/codex.js", {}, driftedVersionRunner),
    (error: unknown) =>
      error instanceof CodexCanaryError && error.code === "cli_command_failed",
  );
});

test("CLI inspection maps a nonzero logged-out status to an actionable error", async () => {
  const loggedOutRunner: ProcessRunner = async (_executable, args) => ({
    exitCode: args.at(-1) === "--version" ? 0 : 1,
    stdout: args.at(-1) === "--version" ? "codex-cli 0.144.5\n" : "",
    stderr: args.at(-1) === "--version" ? "" : "Not logged in\n",
  });

  await assert.rejects(
    inspectCodexCli("C:/synthetic/codex.js", {}, loggedOutRunner),
    (error: unknown) =>
      error instanceof CodexCanaryError &&
      error.code === "chatgpt_login_required",
  );
});

test("runtime readiness verifies the pinned CLI and ChatGPT login", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "dennett-runtime-auth-"));
  try {
    const environment = { LOCALAPPDATA: root, Path: "C:/Windows/System32" };
    const runtimeHome = path.join(root, "Dennett", "codex-runtime");
    await mkdir(runtimeHome, { recursive: true });
    await writeFile(path.join(runtimeHome, "auth.json"), "{}", "utf8");
    const calls: Array<{ args: string[]; environment: Record<string, string> }> = [];
    const runner: ProcessRunner = async (_executable, args, options) => {
      calls.push({ args, environment: options.environment });
      return {
        exitCode: 0,
        stdout: args.at(-1) === "--version" ? "codex-cli 0.144.5\n" : "",
        stderr: args.at(-1) === "--version" ? "" : "Logged in using ChatGPT\n",
      };
    };

    assert.equal(
      await prepareRuntimeCodexHome(environment, {
        installation: {
          launcherPath: "C:/synthetic/codex.js",
          sdkVersion: "0.144.5",
        },
        runner,
      }),
      runtimeHome,
    );
    assert.equal(calls.length, 2);
    assert.equal(calls[0]?.environment.CODEX_HOME, runtimeHome);
    assert.equal("OPENAI_API_KEY" in (calls[0]?.environment ?? {}), false);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
