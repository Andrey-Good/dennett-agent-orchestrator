import path from "node:path";

import {
  assertNoApiKeyEnvironment,
  createSanitizedCodexEnvironment,
  loadRuntimeCodexModelCatalog,
  prepareRuntimeCodexHome,
  resolveCodexInstallation,
  subscriptionCliArguments,
} from "./codex-cli.js";
import { CodexAppServerClient } from "./codex-app-server-client.js";
import {
  codexAccessConfiguration,
  codexAccessThreadOptions,
  codexRuntimeConfiguration,
} from "./codex-model-catalog.js";
import { CodexCanaryError } from "./codex-canary-lib.js";
import { CodexRuntimeAdapter } from "./codex-runtime-adapter.js";
import {
  RuntimeAdapterError,
  type AgentRuntimeAdapter,
  type CancelRuntimeTurnRequest,
  type CancellationAcknowledgement,
  type RuntimeDescriptor,
  type SteerRuntimeTurnRequest,
  type SteeringAcknowledgement,
  type RuntimeTurn,
  type RuntimeTurnRequest,
} from "./runtime-contract.js";
import { RuntimeHost } from "./runtime-host.js";

assertNoApiKeyEnvironment(process.env);

const MAX_HOST_MESSAGE_BYTES = 1024 * 1024;
const MAX_IN_FLIGHT_HOST_REQUESTS = 64;

class LazyCodexRuntimeAdapter implements AgentRuntimeAdapter {
  #adapter: Promise<CodexRuntimeAdapter> | undefined;

  async describe(): Promise<RuntimeDescriptor> {
    return (await this.adapter()).describe();
  }

  async startTurn(request: RuntimeTurnRequest): Promise<RuntimeTurn> {
    return (await this.adapter()).startTurn(request);
  }

  async steerTurn(
    request: SteerRuntimeTurnRequest,
  ): Promise<SteeringAcknowledgement> {
    if (!this.#adapter) {
      throw new RuntimeAdapterError("scope_mismatch", false, true);
    }
    return (await this.#adapter).steerTurn(request);
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

  async close(): Promise<void> {
    if (this.#adapter) await (await this.#adapter).close();
  }

  private adapter(): Promise<CodexRuntimeAdapter> {
    this.#adapter ??= Promise.all([
      prepareRuntimeCodexHome(process.env),
      resolveCodexInstallation(),
    ])
      .then(async ([codexHome, installation]) => {
        const environment = createSanitizedCodexEnvironment(process.env);
        environment.CODEX_HOME = codexHome;
        environment.PATH = installation.nativePathDirectories
          .concat(environment.PATH ? [environment.PATH] : [])
          .join(path.delimiter);
        const platformArguments = process.platform === "win32"
          ? ["--config", 'windows.sandbox="unelevated"']
          : [];
        const baseClient = await CodexAppServerClient.start({
          executablePath: installation.nativeExecutablePath,
          environment,
          cliArguments: subscriptionCliArguments([
            ...platformArguments,
            "app-server",
            "--stdio",
          ]),
        });
        return loadRuntimeCodexModelCatalog(process.env, codexHome)
          .then((catalog) => {
            const configuration = codexRuntimeConfiguration(catalog);
            return new CodexRuntimeAdapter(baseClient, {
              steering: "native",
              controls: configuration.controls,
              resolveRuntimeControls: (selections) => {
                const resolved = configuration.resolve(selections);
                return {
                  client: baseClient.withDefaults({
                    reasoningEffort: resolved.reasoningEffort,
                    serviceTier: resolved.serviceTier,
                  }),
                  threadOptions: {
                    model: resolved.model,
                    ...codexAccessThreadOptions(resolved.accessMode),
                  },
                };
              },
            });
          })
          .catch(() => {
            const access = codexAccessConfiguration();
            return new CodexRuntimeAdapter(baseClient, {
              steering: "native",
              controls: access.controls,
              resolveRuntimeControls: (selections) => ({
                client: baseClient,
                threadOptions: codexAccessThreadOptions(access.resolve(selections)),
              }),
            });
          });
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
  const inFlight = new Set<Promise<void>>();
  const dispatch = async (line: string): Promise<void> => {
    if (inFlight.size >= MAX_IN_FLIGHT_HOST_REQUESTS) {
      await Promise.race(inFlight);
    }
    const handling = host.handleLine(line);
    inFlight.add(handling);
    void handling.then(
      () => inFlight.delete(handling),
      () => {
        inFlight.delete(handling);
        process.exitCode = 1;
      },
    );
  };
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
      await dispatch(pending.toString("utf8"));
      pending = Buffer.alloc(0);
      offset = newline + 1;
    }
  }
  if (pending.length > 0) {
    await dispatch(pending.toString("utf8"));
  }
  await Promise.allSettled(inFlight);
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
