import { execFile } from "node:child_process";
import { randomUUID } from "node:crypto";
import { link, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

import { CodexCanaryError } from "./codex-canary-lib.js";

export interface IsolatedCanaryWorkspace {
  rootDirectory: string;
  workingDirectory: string;
  codexHomeDirectory: string;
  gitConfigFile: string;
  osHomeDirectory: string;
  cleanup(): Promise<void>;
}

const GIT_INIT_TIMEOUT_MS = 10_000;
const FILESYSTEM_OPERATION_TIMEOUT_MS = 10_000;

export async function withFilesystemDeadline<T>(
  operation: Promise<T>,
  errorCode: "workspace_cleanup_failed" | "workspace_setup_failed",
  onLateSettlement?: () => Promise<void>,
  timeoutMs = FILESYSTEM_OPERATION_TIMEOUT_MS,
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const deadline = new Promise<{ kind: "timeout" }>((resolve) => {
    timer = setTimeout(() => resolve({ kind: "timeout" }), timeoutMs);
  });
  const observedOperation = operation.then(
    (value) => ({ kind: "value" as const, value }),
    (error: unknown) => ({ kind: "error" as const, error }),
  );
  try {
    const outcome = await Promise.race([observedOperation, deadline]);
    if (outcome.kind === "timeout") {
      if (onLateSettlement !== undefined) {
        void observedOperation
          .then(() => onLateSettlement())
          .catch(() => undefined);
      }
      throw new CodexCanaryError(
        errorCode,
        onLateSettlement === undefined ? undefined : { cleanupFailed: true },
      );
    }
    if (outcome.kind === "error") {
      throw outcome.error;
    }
    return outcome.value;
  } finally {
    if (timer !== undefined) {
      clearTimeout(timer);
    }
  }
}

function isolatedGitEnvironment(
  globalConfigFile: string,
): NodeJS.ProcessEnv {
  const environment: NodeJS.ProcessEnv = {};
  for (const [name, value] of Object.entries(process.env)) {
    if (!name.toUpperCase().startsWith("GIT_") && value !== undefined) {
      environment[name] = value;
    }
  }
  environment.GIT_CONFIG_GLOBAL = globalConfigFile;
  environment.GIT_CONFIG_COUNT = "0";
  environment.GIT_CONFIG_NOSYSTEM = "1";
  return environment;
}

function initializeGitRepository(
  workingDirectory: string,
  templateDirectory: string,
  globalConfigFile: string,
): Promise<void> {
  return new Promise((resolve, reject) => {
    execFile(
      "git",
      ["init", "--quiet", `--template=${templateDirectory}`],
      {
        cwd: workingDirectory,
        env: isolatedGitEnvironment(globalConfigFile),
        timeout: GIT_INIT_TIMEOUT_MS,
        windowsHide: true,
      },
      (error) => {
        if (error) {
          reject(new CodexCanaryError("workspace_setup_failed"));
          return;
        }
        resolve();
      },
    );
  });
}

async function removeCanaryPaths(paths: string[]): Promise<boolean> {
  const outcomes = await Promise.all(
    paths.map(async (target) => {
      const removeOnce = (): Promise<void> =>
        rm(target, {
          force: true,
          recursive: true,
          maxRetries: 10,
          retryDelay: 100,
        });
      const reconcileLateRemoval = async (): Promise<void> => {
        await withFilesystemDeadline(
          removeOnce(),
          "workspace_cleanup_failed",
        );
      };
      try {
        await withFilesystemDeadline(
          removeOnce(),
          "workspace_cleanup_failed",
          reconcileLateRemoval,
        );
        return true;
      } catch {
        return false;
      }
    }),
  );
  return outcomes.every(Boolean);
}

export async function createIsolatedCanaryWorkspace(
  sourceAuthFile: string,
): Promise<IsolatedCanaryWorkspace> {
  const rootDirectory = path.join(
    tmpdir(),
    `dennett-codex-canary-${randomUUID()}`,
  );
  const workingDirectory = path.join(rootDirectory, "workspace");
  const gitTemplateDirectory = path.join(rootDirectory, "git-template");
  const gitConfigFile = path.join(rootDirectory, "gitconfig");
  const osHomeDirectory = path.join(rootDirectory, "home");
  const codexHomeDirectory = path.join(
    path.dirname(sourceAuthFile),
    `.dennett-canary-run-${randomUUID()}`,
  );
  const reconcileLateSetup = async (): Promise<void> => {
    await removeCanaryPaths([codexHomeDirectory, rootDirectory]);
  };

  try {
    await withFilesystemDeadline(
      mkdir(rootDirectory),
      "workspace_setup_failed",
      reconcileLateSetup,
    );
    await withFilesystemDeadline(
      mkdir(codexHomeDirectory),
      "workspace_setup_failed",
      reconcileLateSetup,
    );
    await withFilesystemDeadline(
      mkdir(workingDirectory),
      "workspace_setup_failed",
      reconcileLateSetup,
    );
    await withFilesystemDeadline(
      mkdir(gitTemplateDirectory),
      "workspace_setup_failed",
      reconcileLateSetup,
    );
    await withFilesystemDeadline(
      mkdir(osHomeDirectory),
      "workspace_setup_failed",
      reconcileLateSetup,
    );
    await withFilesystemDeadline(
      writeFile(gitConfigFile, "", "utf8"),
      "workspace_setup_failed",
      reconcileLateSetup,
    );
    await withFilesystemDeadline(
      link(sourceAuthFile, path.join(codexHomeDirectory, "auth.json")),
      "workspace_setup_failed",
      reconcileLateSetup,
    );
    await initializeGitRepository(
      workingDirectory,
      gitTemplateDirectory,
      gitConfigFile,
    );
    await withFilesystemDeadline(
      rm(gitTemplateDirectory, { recursive: true }),
      "workspace_setup_failed",
      reconcileLateSetup,
    );
  } catch (error: unknown) {
    const cleanupFailed = !(await removeCanaryPaths([
      codexHomeDirectory,
      rootDirectory,
    ]));
    if (error instanceof CodexCanaryError) {
      throw new CodexCanaryError(error.code, {
        ...error.safeDetail,
        ...(cleanupFailed ? { cleanupFailed: true } : {}),
      });
    }
    throw new CodexCanaryError(
      "workspace_setup_failed",
      cleanupFailed ? { cleanupFailed: true } : undefined,
    );
  }

  let cleaned = false;
  return {
    rootDirectory,
    workingDirectory,
    codexHomeDirectory,
    gitConfigFile,
    osHomeDirectory,
    async cleanup(): Promise<void> {
      if (cleaned) {
        return;
      }
      if (
        await removeCanaryPaths([codexHomeDirectory, rootDirectory])
      ) {
        cleaned = true;
        return;
      }
      throw new CodexCanaryError("workspace_cleanup_failed", {
        cleanupFailed: true,
      });
    },
  };
}
