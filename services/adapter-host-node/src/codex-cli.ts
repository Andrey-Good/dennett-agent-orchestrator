import { execFile } from "node:child_process";
import { access, link, mkdir, readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

import type { CodexOptions } from "@openai/codex-sdk";

import { CodexCanaryError } from "./codex-canary-lib.js";

const CHATGPT_BASE_URL = "https://chatgpt.com/backend-api/";
export const PINNED_CODEX_VERSION = "0.144.6";
const CLI_FILESYSTEM_TIMEOUT_MS = 10_000;
const MAX_MODEL_CATALOG_BYTES = 8 * 1024 * 1024;
const CHATGPT_LOGIN_STATUS = "Logged in using ChatGPT";
const SUBSCRIPTION_CONFIG_ARGUMENTS = [
  "--config",
  'model_provider="openai"',
  "--config",
  'forced_login_method="chatgpt"',
  "--config",
  `chatgpt_base_url="${CHATGPT_BASE_URL}"`,
  "--config",
  'cli_auth_credentials_store="file"',
  "--config",
  'history.persistence="none"',
];
const FORBIDDEN_AUTH_ENVIRONMENT = new Set([
  "OPENAI_API_KEY",
  "CODEX_API_KEY",
  "CODEX_ACCESS_TOKEN",
]);
const SAFE_ENVIRONMENT = new Set([
  "ALLUSERSPROFILE",
  "APPDATA",
  "COMSPEC",
  "HOME",
  "HOMEDRIVE",
  "HOMEPATH",
  "LANG",
  "LC_ALL",
  "LOCALAPPDATA",
  "NUMBER_OF_PROCESSORS",
  "OS",
  "PATH",
  "PATHEXT",
  "PROGRAMDATA",
  "PROGRAMFILES",
  "PROGRAMFILES(X86)",
  "PROGRAMW6432",
  "SYSTEMDRIVE",
  "SYSTEMROOT",
  "TEMP",
  "TERM",
  "TMP",
  "USERDOMAIN",
  "USERNAME",
  "USERPROFILE",
  "WINDIR",
]);

export interface CodexInstallation {
  launcherPath: string;
  nativeExecutablePath: string;
  nativePathDirectories: string[];
  sdkVersion: string;
}

function nativeCodexTarget(): { packageName: string; triple: string } {
  const target = `${process.platform}-${process.arch}`;
  const targets: Record<string, { packageName: string; triple: string }> = {
    "linux-x64": { packageName: "@openai/codex-linux-x64", triple: "x86_64-unknown-linux-musl" },
    "linux-arm64": { packageName: "@openai/codex-linux-arm64", triple: "aarch64-unknown-linux-musl" },
    "darwin-x64": { packageName: "@openai/codex-darwin-x64", triple: "x86_64-apple-darwin" },
    "darwin-arm64": { packageName: "@openai/codex-darwin-arm64", triple: "aarch64-apple-darwin" },
    "win32-x64": { packageName: "@openai/codex-win32-x64", triple: "x86_64-pc-windows-msvc" },
    "win32-arm64": { packageName: "@openai/codex-win32-arm64", triple: "aarch64-pc-windows-msvc" },
  };
  const resolved = targets[target];
  if (!resolved) throw new CodexCanaryError("codex_package_invalid");
  return resolved;
}

export interface CodexCliStatus {
  authMode: "chatgpt";
  cliVersion: string;
}

export interface ProcessResult {
  exitCode: number;
  stdout: string;
  stderr: string;
}

export type ProcessRunner = (
  executable: string,
  args: string[],
  options: { environment: Record<string, string> },
) => Promise<ProcessResult>;

async function withCliFilesystemDeadline<T>(
  operation: Promise<T>,
  errorCode:
    | "chatgpt_login_required"
    | "codex_binary_missing"
    | "codex_package_invalid",
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const deadline = new Promise<never>((_resolve, reject) => {
    timer = setTimeout(
      () => reject(new CodexCanaryError(errorCode)),
      CLI_FILESYSTEM_TIMEOUT_MS,
    );
  });
  try {
    return await Promise.race([operation, deadline]);
  } finally {
    if (timer !== undefined) {
      clearTimeout(timer);
    }
  }
}

const runProcess: ProcessRunner = (executable, args, options) =>
  new Promise((resolve, reject) => {
    execFile(
      executable,
      args,
      {
        encoding: "utf8",
        env: options.environment,
        timeout: 30_000,
        windowsHide: true,
      },
      (error, stdout, stderr) => {
        if (!error) {
          resolve({ exitCode: 0, stdout, stderr });
          return;
        }
        if (typeof error.code === "number") {
          resolve({ exitCode: error.code, stdout, stderr });
          return;
        }
        reject(
          new CodexCanaryError(
            error.code === "ENOENT"
              ? "codex_binary_missing"
              : "cli_command_failed",
          ),
        );
      },
    );
  });

export function assertNoApiKeyEnvironment(
  environment: NodeJS.ProcessEnv,
): void {
  for (const [name, value] of Object.entries(environment)) {
    if (
      FORBIDDEN_AUTH_ENVIRONMENT.has(name.toUpperCase()) &&
      Boolean(value?.trim())
    ) {
      throw new CodexCanaryError("api_key_environment_present");
    }
  }
}

export function createSanitizedCodexEnvironment(
  environment: NodeJS.ProcessEnv,
): Record<string, string> {
  const sanitized: Record<string, string> = {};
  for (const [name, value] of Object.entries(environment)) {
    if (value !== undefined && SAFE_ENVIRONMENT.has(name.toUpperCase())) {
      sanitized[name.toUpperCase()] = value;
    }
  }
  return sanitized;
}

export function createSubscriptionCodexOptions(
  environment: NodeJS.ProcessEnv,
  isolation: {
    codexHomeDirectory: string;
    gitConfigFile: string;
    osHomeDirectory: string;
    workingDirectory: string;
  },
): CodexOptions {
  const sanitizedEnvironment = createSanitizedCodexEnvironment(environment);
  delete sanitizedEnvironment.HOMEDRIVE;
  delete sanitizedEnvironment.HOMEPATH;
  sanitizedEnvironment.HOME = isolation.osHomeDirectory;
  sanitizedEnvironment.USERPROFILE = isolation.osHomeDirectory;
  sanitizedEnvironment.CODEX_HOME = isolation.codexHomeDirectory;
  sanitizedEnvironment.GIT_CEILING_DIRECTORIES = isolation.workingDirectory;
  sanitizedEnvironment.GIT_CONFIG_COUNT = "0";
  sanitizedEnvironment.GIT_CONFIG_GLOBAL = isolation.gitConfigFile;
  sanitizedEnvironment.GIT_CONFIG_NOSYSTEM = "1";
  return {
    env: sanitizedEnvironment,
    config: {
      model_provider: "openai",
      forced_login_method: "chatgpt",
      chatgpt_base_url: CHATGPT_BASE_URL,
      cli_auth_credentials_store: "file",
      history: { persistence: "none" },
    },
  };
}

export function subscriptionCliArguments(command: string[]): string[] {
  return [...SUBSCRIPTION_CONFIG_ARGUMENTS, ...command];
}

export function resolveCanaryCodexHomeDirectory(
  environment: NodeJS.ProcessEnv,
): string {
  if (environment.LOCALAPPDATA) {
    return path.join(
      environment.LOCALAPPDATA,
      "Dennett",
      "codex-canary-auth",
    );
  }
  const homeDirectory = environment.USERPROFILE ?? environment.HOME;
  if (!homeDirectory) {
    throw new CodexCanaryError("chatgpt_login_required");
  }
  return path.join(homeDirectory, ".dennett", "codex-canary-auth");
}

export function resolveRuntimeCodexHomeDirectory(
  environment: NodeJS.ProcessEnv,
): string {
  if (environment.LOCALAPPDATA) {
    return path.join(environment.LOCALAPPDATA, "Dennett", "codex-runtime");
  }
  const homeDirectory = environment.USERPROFILE ?? environment.HOME;
  if (!homeDirectory) {
    throw new CodexCanaryError("chatgpt_login_required");
  }
  return path.join(homeDirectory, ".dennett", "codex-runtime");
}

export async function prepareRuntimeCodexHome(
  environment: NodeJS.ProcessEnv,
  inspection: {
    installation?: CodexInstallation;
    runner?: ProcessRunner;
  } = {},
): Promise<string> {
  const runtimeHome = resolveRuntimeCodexHomeDirectory(environment);
  const runtimeAuth = path.join(runtimeHome, "auth.json");
  await mkdir(runtimeHome, { recursive: true });
  try {
    await access(runtimeAuth);
  } catch {
    const canaryAuth = await resolveCanaryCodexAuthFile(environment);
    try {
      await link(canaryAuth, runtimeAuth);
    } catch (error: unknown) {
      if (
        typeof error !== "object" ||
        error === null ||
        !("code" in error) ||
        error.code !== "EEXIST"
      ) {
        throw new CodexCanaryError("workspace_setup_failed");
      }
    }
  }

  const installation =
    inspection.installation ?? (await resolveCodexInstallation());
  const runtimeEnvironment = createSanitizedCodexEnvironment(environment);
  runtimeEnvironment.CODEX_HOME = runtimeHome;
  await inspectCodexCli(
    installation.launcherPath,
    runtimeEnvironment,
    inspection.runner,
  );
  return runtimeHome;
}

export function createRuntimeSubscriptionCodexOptions(
  environment: NodeJS.ProcessEnv,
  codexHomeDirectory: string,
  configOverrides: NonNullable<CodexOptions["config"]> = {},
  platform: NodeJS.Platform = process.platform,
): CodexOptions {
  const sanitizedEnvironment = createSanitizedCodexEnvironment(environment);
  sanitizedEnvironment.CODEX_HOME = codexHomeDirectory;
  return {
    env: sanitizedEnvironment,
    config: {
      model_provider: "openai",
      forced_login_method: "chatgpt",
      chatgpt_base_url: CHATGPT_BASE_URL,
      cli_auth_credentials_store: "file",
      // Codex otherwise degrades workspace-write to read-only on native Windows.
      // The unelevated restricted-token sandbox preserves the workspace boundary.
      ...(platform === "win32" ? { windows: { sandbox: "unelevated" } } : {}),
      ...configOverrides,
    },
  };
}

export async function loadRuntimeCodexModelCatalog(
  environment: NodeJS.ProcessEnv,
  codexHomeDirectory: string,
  inspection: {
    installation?: CodexInstallation;
    runner?: ProcessRunner;
  } = {},
): Promise<unknown> {
  const installation = inspection.installation ?? (await resolveCodexInstallation());
  const runtimeEnvironment = createSanitizedCodexEnvironment(environment);
  runtimeEnvironment.CODEX_HOME = codexHomeDirectory;
  const result = await (inspection.runner ?? runProcess)(
    process.execPath,
    [installation.launcherPath, ...subscriptionCliArguments(["debug", "models"])],
    { environment: runtimeEnvironment },
  );
  if (
    result.exitCode !== 0 ||
    Buffer.byteLength(result.stdout, "utf8") > MAX_MODEL_CATALOG_BYTES
  ) {
    throw new CodexCanaryError("cli_command_failed");
  }
  try {
    return JSON.parse(result.stdout) as unknown;
  } catch {
    throw new CodexCanaryError("cli_command_failed");
  }
}

export async function resolveCanaryCodexAuthFile(
  environment: NodeJS.ProcessEnv,
): Promise<string> {
  const authFile = path.join(
    resolveCanaryCodexHomeDirectory(environment),
    "auth.json",
  );
  try {
    await withCliFilesystemDeadline(
      access(authFile),
      "chatgpt_login_required",
    );
  } catch {
    throw new CodexCanaryError("chatgpt_login_required");
  }
  return authFile;
}

export async function resolveCodexInstallation(): Promise<CodexInstallation> {
  try {
    const sdkEntry = fileURLToPath(import.meta.resolve("@openai/codex-sdk"));
    const sdkPackagePath = path.resolve(
      path.dirname(sdkEntry),
      "..",
      "package.json",
    );
    const sdkPackage = JSON.parse(
      await withCliFilesystemDeadline(
        readFile(sdkPackagePath, "utf8"),
        "codex_package_invalid",
      ),
    ) as { version?: unknown };
    if (sdkPackage.version !== PINNED_CODEX_VERSION) {
      throw new CodexCanaryError("codex_package_invalid");
    }

    const sdkRequire = createRequire(sdkEntry);
    const codexPackagePath = sdkRequire.resolve("@openai/codex/package.json");
    const codexPackage = JSON.parse(
      await withCliFilesystemDeadline(
        readFile(codexPackagePath, "utf8"),
        "codex_package_invalid",
      ),
    ) as {
      bin?: unknown;
      version?: unknown;
    };
    if (codexPackage.version !== PINNED_CODEX_VERSION) {
      throw new CodexCanaryError("codex_package_invalid");
    }
    const launcherRelativePath =
      typeof codexPackage.bin === "object" &&
      codexPackage.bin !== null &&
      "codex" in codexPackage.bin &&
      typeof (codexPackage.bin as { codex?: unknown }).codex === "string"
        ? (codexPackage.bin as { codex: string }).codex
        : null;
    if (!launcherRelativePath) {
      throw new CodexCanaryError("codex_package_invalid");
    }

    const launcherPath = path.resolve(
      path.dirname(codexPackagePath),
      launcherRelativePath,
    );
    const nativeTarget = nativeCodexTarget();
    const nativePackagePath = sdkRequire.resolve(`${nativeTarget.packageName}/package.json`);
    const nativeRoot = path.resolve(
      path.dirname(nativePackagePath),
      "vendor",
      nativeTarget.triple,
    );
    const nativeExecutablePath = path.join(
      nativeRoot,
      "bin",
      process.platform === "win32" ? "codex.exe" : "codex",
    );
    const nativePathDirectories = [path.join(nativeRoot, "codex-path")];
    await withCliFilesystemDeadline(
      Promise.all([access(launcherPath), access(nativeExecutablePath), access(nativePathDirectories[0])]),
      "codex_binary_missing",
    );
    return {
      launcherPath,
      nativeExecutablePath,
      nativePathDirectories,
      sdkVersion: PINNED_CODEX_VERSION,
    };
  } catch (error: unknown) {
    if (error instanceof CodexCanaryError) {
      throw error;
    }
    if (
      typeof error === "object" &&
      error !== null &&
      "code" in error &&
      error.code === "ENOENT"
    ) {
      throw new CodexCanaryError("codex_binary_missing");
    }
    throw new CodexCanaryError("codex_package_invalid");
  }
}

export async function inspectCodexCli(
  launcherPath: string,
  environment: Record<string, string>,
  runner: ProcessRunner = runProcess,
): Promise<CodexCliStatus> {
  const version = await runner(
    process.execPath,
    [launcherPath, "--version"],
    { environment },
  );
  if (version.exitCode !== 0) {
    throw new CodexCanaryError("cli_command_failed");
  }

  const login = await runner(
    process.execPath,
    [launcherPath, ...subscriptionCliArguments(["login", "status"])],
    { environment },
  );
  const loginStatus = `${login.stdout}\n${login.stderr}`;
  if (login.exitCode !== 0) {
    if (/not logged in|login required|please log in/i.test(loginStatus)) {
      throw new CodexCanaryError("chatgpt_login_required");
    }
    throw new CodexCanaryError("cli_command_failed");
  }
  const loginLines = loginStatus
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
  if (
    loginLines.length !== 1 ||
    loginLines[0] !== CHATGPT_LOGIN_STATUS ||
    /api key|personal access token|bedrock|custom provider/i.test(loginStatus)
  ) {
    throw new CodexCanaryError("chatgpt_login_required");
  }

  const versionLines = `${version.stdout}\n${version.stderr}`
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
  const normalizedVersion = `codex-cli ${PINNED_CODEX_VERSION}`;
  if (versionLines.length !== 1 || versionLines[0] !== normalizedVersion) {
    throw new CodexCanaryError("cli_command_failed");
  }

  return {
    authMode: "chatgpt",
    cliVersion: normalizedVersion,
  };
}
