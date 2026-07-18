import { spawn } from "node:child_process";
import { mkdir } from "node:fs/promises";

import { CodexCanaryError } from "./codex-canary-lib.js";
import {
  assertNoApiKeyEnvironment,
  createSanitizedCodexEnvironment,
  inspectCodexCli,
  resolveCanaryCodexHomeDirectory,
  resolveCodexInstallation,
  subscriptionCliArguments,
} from "./codex-cli.js";

const LOGIN_TIMEOUT_MS = 10 * 60_000;

function runInteractiveLogin(
  launcherPath: string,
  environment: Record<string, string>,
): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn(
      process.execPath,
      [launcherPath, ...subscriptionCliArguments(["login"])],
      {
        env: environment,
        stdio: "inherit",
        windowsHide: true,
      },
    );
    const timer = setTimeout(() => {
      child.kill();
      reject(new CodexCanaryError("cli_command_failed"));
    }, LOGIN_TIMEOUT_MS);
    child.once("error", () => {
      clearTimeout(timer);
      reject(new CodexCanaryError("cli_command_failed"));
    });
    child.once("exit", (code) => {
      clearTimeout(timer);
      if (code === 0) {
        resolve();
        return;
      }
      reject(new CodexCanaryError("cli_command_failed"));
    });
  });
}

async function main(): Promise<void> {
  assertNoApiKeyEnvironment(process.env);
  const installation = await resolveCodexInstallation();
  const codexHomeDirectory = resolveCanaryCodexHomeDirectory(process.env);
  await mkdir(codexHomeDirectory, { recursive: true });

  const environment = createSanitizedCodexEnvironment(process.env);
  environment.CODEX_HOME = codexHomeDirectory;
  await runInteractiveLogin(installation.launcherPath, environment);
  await inspectCodexCli(installation.launcherPath, environment);
  process.stdout.write("Dennett Codex canary ChatGPT login is ready.\n");
}

main().catch((error: unknown) => {
  const code =
    error instanceof CodexCanaryError ? error.code : "unexpected_canary_failure";
  process.stderr.write(`${JSON.stringify({ status: "failed", code })}\n`);
  process.exitCode = 1;
});
