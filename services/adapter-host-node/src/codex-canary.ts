import { Codex } from "@openai/codex-sdk";

import {
  CodexCanaryError,
  CANARY_THREAD_OPTIONS,
} from "./codex-canary-lib.js";
import { CodexRuntimeAdapter } from "./codex-runtime-adapter.js";
import { runRuntimeAdapterCanary } from "./codex-runtime-canary.js";
import {
  assertNoApiKeyEnvironment,
  createSubscriptionCodexOptions,
  inspectCodexCli,
  resolveCanaryCodexAuthFile,
  resolveCodexInstallation,
} from "./codex-cli.js";
import { createIsolatedCanaryWorkspace } from "./codex-workspace.js";

const FIRST_PROMPT =
  "This is a synthetic connectivity canary. Reply with one short acknowledgement. Do not inspect files, call tools, or describe the workspace.";
const CONTINUATION_PROMPT =
  "Reply with one more short acknowledgement to prove this same thread continues. Do not call tools.";

async function main(): Promise<void> {
  assertNoApiKeyEnvironment(process.env);
  const installation = await resolveCodexInstallation();
  const authFile = await resolveCanaryCodexAuthFile(process.env);
  const workspace = await createIsolatedCanaryWorkspace(authFile);

  let output: string | undefined;
  let failure: unknown;
  try {
    const codexOptions = createSubscriptionCodexOptions(
      process.env,
      workspace,
    );
    const cli = await inspectCodexCli(
      installation.launcherPath,
      codexOptions.env ?? {},
    );
    const codex = new Codex(codexOptions);
    const adapter = new CodexRuntimeAdapter(codex, {
      threadOptions: CANARY_THREAD_OPTIONS,
    });
    const report = await runRuntimeAdapterCanary(adapter, {
      workingDirectory: workspace.workingDirectory,
      firstPrompt: FIRST_PROMPT,
      continuationPrompt: CONTINUATION_PROMPT,
    });

    output = `${JSON.stringify({
      status: "passed",
      authMode: cli.authMode,
      sdkVersion: installation.sdkVersion,
      cliVersion: cli.cliVersion,
      ...report,
    })}\n`;
  } catch (error: unknown) {
    failure = error;
  }

  try {
    await workspace.cleanup();
  } catch (cleanupError: unknown) {
    failure =
      failure instanceof CodexCanaryError
        ? new CodexCanaryError(failure.code, {
            ...failure.safeDetail,
            cleanupFailed: true,
          })
        : cleanupError;
  }

  if (failure !== undefined) {
    throw failure;
  }
  if (output === undefined) {
    throw new Error("canary output missing");
  }
  process.stdout.write(output);
}

main().catch((error: unknown) => {
  const code =
    error instanceof CodexCanaryError ? error.code : "unexpected_canary_failure";
  const detail = error instanceof CodexCanaryError ? error.safeDetail : undefined;
  process.stderr.write(
    `${JSON.stringify({ status: "failed", code, ...(detail ?? {}) })}\n`,
  );
  process.exitCode = 1;
});
