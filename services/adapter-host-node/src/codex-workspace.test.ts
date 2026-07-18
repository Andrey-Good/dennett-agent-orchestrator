import assert from "node:assert/strict";
import {
  access,
  mkdtemp,
  readFile,
  readdir,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";

import { CodexCanaryError } from "./codex-canary-lib.js";
import {
  createIsolatedCanaryWorkspace,
  withFilesystemDeadline,
} from "./codex-workspace.js";

test("timed-out setup schedules reconciliation after a late operation", async () => {
  let finishOperation: (() => void) | undefined;
  let reconciled = false;
  const operation = new Promise<void>((resolve) => {
    finishOperation = resolve;
  });

  await assert.rejects(
    withFilesystemDeadline(
      operation,
      "workspace_setup_failed",
      async () => {
        reconciled = true;
      },
      10,
    ),
    (error: unknown) =>
      error instanceof CodexCanaryError &&
      error.code === "workspace_setup_failed" &&
      error.safeDetail?.cleanupFailed === true,
  );

  assert.ok(finishOperation);
  finishOperation();
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(reconciled, true);
});

test("timed-out cleanup schedules a bounded late removal pass", async () => {
  let finishRemoval: (() => void) | undefined;
  let reconciliationPasses = 0;
  const removal = new Promise<void>((resolve) => {
    finishRemoval = resolve;
  });

  await assert.rejects(
    withFilesystemDeadline(
      removal,
      "workspace_cleanup_failed",
      async () => {
        reconciliationPasses += 1;
      },
      10,
    ),
    (error: unknown) =>
      error instanceof CodexCanaryError &&
      error.code === "workspace_cleanup_failed" &&
      error.safeDetail?.cleanupFailed === true,
  );

  assert.ok(finishRemoval);
  finishRemoval();
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(reconciliationPasses, 1);
});

test("canary workspace contains no project content and is removed after use", async () => {
  const authSourceDirectory = await mkdtemp(
    path.join(tmpdir(), "dennett-codex-auth-fixture-"),
  );
  const authSourceFile = path.join(authSourceDirectory, "auth.json");
  await writeFile(authSourceFile, '{"auth_mode":"chatgpt"}', "utf8");
  const workspace = await createIsolatedCanaryWorkspace(authSourceFile);
  const workspaceRootDirectory = workspace.rootDirectory;
  const codexHomeDirectory = workspace.codexHomeDirectory;
  try {
    try {
      assert.equal(
        path.dirname(workspace.workingDirectory),
        workspaceRootDirectory,
      );
      assert.equal(path.dirname(codexHomeDirectory), authSourceDirectory);
      assert.deepEqual(
        (await readdir(workspaceRootDirectory)).sort(),
        ["gitconfig", "home", "workspace"],
      );
      assert.deepEqual(await readdir(workspace.workingDirectory), [".git"]);
      assert.deepEqual(
        (await readdir(path.join(workspace.workingDirectory, ".git"))).sort(),
        ["HEAD", "config", "objects", "refs"],
      );
      assert.deepEqual(await readdir(codexHomeDirectory), ["auth.json"]);
      assert.equal(await readFile(workspace.gitConfigFile, "utf8"), "");
      assert.deepEqual(await readdir(workspace.osHomeDirectory), []);
      assert.equal(
        await readFile(path.join(codexHomeDirectory, "auth.json"), "utf8"),
        '{"auth_mode":"chatgpt"}',
      );
    } finally {
      await workspace.cleanup();
    }

    await assert.rejects(access(workspaceRootDirectory));
    await assert.rejects(access(codexHomeDirectory));
    assert.equal(
      await readFile(authSourceFile, "utf8"),
      '{"auth_mode":"chatgpt"}',
    );
  } finally {
    await rm(authSourceDirectory, { force: true, recursive: true });
  }
});
