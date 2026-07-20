import { Codex } from "@openai/codex-sdk";

import {
  assertNoApiKeyEnvironment,
  createRuntimeSubscriptionCodexOptions,
  prepareRuntimeCodexHome,
} from "./codex-cli.js";
import { CodexCanaryError } from "./codex-canary-lib.js";
import { CodexRuntimeAdapter } from "./codex-runtime-adapter.js";
import {
  RuntimeAdapterError,
  type AgentRuntimeAdapter,
  type CancelRuntimeTurnRequest,
  type CancellationAcknowledgement,
  type RuntimeDescriptor,
  type RuntimeTurn,
  type RuntimeTurnRequest,
} from "./runtime-contract.js";
import { RuntimeHost } from "./runtime-host.js";

assertNoApiKeyEnvironment(process.env);

const MAX_HOST_MESSAGE_BYTES = 1024 * 1024;

class LazyCodexRuntimeAdapter implements AgentRuntimeAdapter {
  #adapter: Promise<CodexRuntimeAdapter> | undefined;

  async describe(): Promise<RuntimeDescriptor> {
    await this.adapter();
    return {
      adapterId: "openai.codex.sdk",
      runtimeKind: "native_agent",
      capabilities: {
        streaming: true,
        continuation: true,
        scopedCancellation: true,
        deadlines: true,
        nativeExtensionSchemas: ["openai.codex.item-status@0.144.5"],
      },
    };
  }

  async startTurn(request: RuntimeTurnRequest): Promise<RuntimeTurn> {
    return (await this.adapter()).startTurn(request);
  }

  async cancelTurn(
    request: CancelRuntimeTurnRequest,
  ): Promise<CancellationAcknowledgement> {
    if (!this.#adapter) {
      return {
        sessionId: request.sessionId,
        turnId: request.turnId,
        disposition: { type: "not_found" },
      };
    }
    return (await this.#adapter).cancelTurn(request);
  }

  private adapter(): Promise<CodexRuntimeAdapter> {
    this.#adapter ??= prepareRuntimeCodexHome(process.env)
      .then((codexHome) => {
        const client = new Codex(
          createRuntimeSubscriptionCodexOptions(process.env, codexHome),
        );
        return new CodexRuntimeAdapter(client);
      })
      .catch((error: unknown) => {
        if (error instanceof CodexCanaryError) {
          throw new RuntimeAdapterError(
            "provider_unavailable",
            error.code === "cli_command_failed",
            true,
          );
        }
        throw error;
      });
    return this.#adapter;
  }
}

let outputTail = Promise.resolve();
const write = (message: Record<string, unknown>): Promise<void> => {
  const encoded = `${JSON.stringify(message)}\n`;
  if (Buffer.byteLength(encoded, "utf8") > MAX_HOST_MESSAGE_BYTES) {
    return Promise.reject(new RuntimeAdapterError("protocol_violation"));
  }
  const current = outputTail.then(
    () =>
      new Promise<void>((resolve, reject) => {
        process.stdout.write(encoded, (error) => {
          if (error) reject(error);
          else resolve();
        });
      }),
  );
  outputTail = current.catch(() => undefined);
  return current;
};

const host = new RuntimeHost(new LazyCodexRuntimeAdapter(), write);

async function consumeInput(): Promise<void> {
  let pending = Buffer.alloc(0);
  for await (const rawChunk of process.stdin) {
    const chunk = Buffer.isBuffer(rawChunk) ? rawChunk : Buffer.from(rawChunk);
    let offset = 0;
    while (offset < chunk.length) {
      const newline = chunk.indexOf(0x0a, offset);
      const end = newline === -1 ? chunk.length : newline;
      const segment = chunk.subarray(offset, end);
      if (pending.length + segment.length > MAX_HOST_MESSAGE_BYTES) {
        throw new RuntimeAdapterError("invalid_request");
      }
      pending = Buffer.concat([pending, segment], pending.length + segment.length);
      if (newline === -1) break;
      if (pending.at(-1) === 0x0d) pending = pending.subarray(0, -1);
      await host.handleLine(pending.toString("utf8"));
      pending = Buffer.alloc(0);
      offset = newline + 1;
    }
  }
  if (pending.length > 0) {
    await host.handleLine(pending.toString("utf8"));
  }
}

void consumeInput()
  .then(() => host.close())
  .then(() => outputTail)
  .catch(() => {
    process.exitCode = 1;
  });

for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.once(signal, () => {
    process.stdin.destroy();
  });
}
