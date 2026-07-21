import {
  RuntimeAdapterError,
  type RuntimeControlDescriptor,
  type RuntimeControlSelection,
} from "./runtime-contract.js";

const DEFAULT_SPEED_ID = "provider_default";
const DEFAULT_REASONING_ID = "provider_default";
export const CODEX_ACCESS_CONTROL_ID = "dennett.access_mode";
const AUTO_APPROVE_ACCESS_ID = "auto_approve";
const FULL_ACCESS_ID = "full_access";
const MODEL_CONTROL_ID = "model";
const REASONING_CONTROL_ID = "reasoning_effort";
const SPEED_CONTROL_ID = "service_tier";

export type CodexAccessMode =
  | typeof AUTO_APPROVE_ACCESS_ID
  | typeof FULL_ACCESS_ID;

interface CodexModel {
  id: string;
  label: string;
  description?: string;
  defaultReasoning: string;
  reasoning: Map<string, string | undefined>;
  serviceTiers: Map<string, { label: string; description?: string }>;
  priority: number;
}

export interface ResolvedCodexControls {
  accessMode: CodexAccessMode;
  model: string;
  reasoningEffort: string;
  serviceTier: string | null;
}

export interface CodexRuntimeConfiguration {
  controls: RuntimeControlDescriptor[];
  resolve(selections: readonly RuntimeControlSelection[]): ResolvedCodexControls;
}

export interface CodexAccessConfiguration {
  controls: RuntimeControlDescriptor[];
  resolve(selections: readonly RuntimeControlSelection[]): CodexAccessMode;
}

function accessControl(): RuntimeControlDescriptor {
  return {
    id: CODEX_ACCESS_CONTROL_ID,
    label: "Agent access",
    defaultChoiceId: AUTO_APPROVE_ACCESS_ID,
    choices: [
      {
        id: AUTO_APPROVE_ACCESS_ID,
        label: "Auto-approve",
        description: "Allow automatic work inside the selected project sandbox",
        availableWhen: [],
      },
      {
        id: FULL_ACCESS_ID,
        label: "Full access",
        description: "Allow commands outside the project sandbox without confirmation",
        availableWhen: [],
      },
    ],
  };
}

function resolveAccessMode(requested: ReadonlyMap<string, string>): CodexAccessMode {
  const accessMode = requested.get(CODEX_ACCESS_CONTROL_ID) ?? AUTO_APPROVE_ACCESS_ID;
  if (accessMode !== AUTO_APPROVE_ACCESS_ID && accessMode !== FULL_ACCESS_ID) {
    throw new RuntimeAdapterError("invalid_request");
  }
  return accessMode;
}

export function codexAccessConfiguration(): CodexAccessConfiguration {
  const controls = [accessControl()];
  return {
    controls,
    resolve(selections) {
      const requested = new Map<string, string>();
      for (const selection of selections) {
        if (
          selection.controlId !== CODEX_ACCESS_CONTROL_ID
          || requested.has(selection.controlId)
        ) {
          throw new RuntimeAdapterError("invalid_request");
        }
        requested.set(selection.controlId, selection.choiceId);
      }
      return resolveAccessMode(requested);
    },
  };
}

export function codexAccessThreadOptions(accessMode: CodexAccessMode): {
  approvalPolicy: "never";
  networkAccessEnabled: true;
  sandboxMode: "workspace-write" | "danger-full-access";
  webSearchMode: "live";
} {
  return {
    approvalPolicy: "never",
    networkAccessEnabled: true,
    sandboxMode: accessMode === FULL_ACCESS_ID
      ? "danger-full-access"
      : "workspace-write",
    webSearchMode: "live",
  };
}

function record(value: unknown): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new RuntimeAdapterError("provider_unavailable", true, true);
  }
  return value as Record<string, unknown>;
}

function text(value: unknown): string {
  if (typeof value !== "string" || value.trim().length === 0 || value.length > 256) {
    throw new RuntimeAdapterError("provider_unavailable", true, true);
  }
  return value;
}

function optionalText(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0
    ? value.slice(0, 1_000)
    : undefined;
}

function labelForReasoning(effort: string): string {
  switch (effort) {
    case "xhigh": return "Extra high";
    case "max": return "Max";
    case "ultra": return "Ultra";
    default: return effort.charAt(0).toUpperCase() + effort.slice(1);
  }
}

function parseModels(value: unknown): CodexModel[] {
  const root = record(value);
  if (!Array.isArray(root.models) || root.models.length > 128) {
    throw new RuntimeAdapterError("provider_unavailable", true, true);
  }
  return root.models
    .map(record)
    .filter((model) => model.visibility === "list")
    .map((model) => {
      const reasoningValues = model.supported_reasoning_levels;
      const serviceTierValues = model.service_tiers ?? [];
      if (
        !Array.isArray(reasoningValues) || reasoningValues.length === 0 || reasoningValues.length > 32 ||
        !Array.isArray(serviceTierValues) || serviceTierValues.length > 32
      ) {
        throw new RuntimeAdapterError("provider_unavailable", true, true);
      }
      const reasoning = new Map(
        reasoningValues.map((value) => {
          const option = record(value);
          return [text(option.effort), optionalText(option.description)] as const;
        }),
      );
      const defaultReasoning = text(model.default_reasoning_level);
      if (!reasoning.has(defaultReasoning) || reasoning.has(DEFAULT_REASONING_ID)) {
        throw new RuntimeAdapterError("provider_unavailable", true, true);
      }
      const serviceTiers = new Map(
        serviceTierValues.map((value) => {
          const option = record(value);
          return [
            text(option.id),
            { label: text(option.name), description: optionalText(option.description) },
          ] as const;
        }),
      );
      if (serviceTiers.has(DEFAULT_SPEED_ID)) {
        throw new RuntimeAdapterError("provider_unavailable", true, true);
      }
      return {
        id: text(model.slug),
        label: text(model.display_name),
        description: optionalText(model.description),
        defaultReasoning,
        reasoning,
        serviceTiers,
        priority: Number.isSafeInteger(model.priority) ? model.priority as number : Number.MAX_SAFE_INTEGER,
      };
    })
    .sort((left, right) => left.priority - right.priority || left.label.localeCompare(right.label));
}

export function codexRuntimeConfiguration(value: unknown): CodexRuntimeConfiguration {
  const models = parseModels(value);
  const defaultModel = models[0];
  if (!defaultModel) {
    throw new RuntimeAdapterError("provider_unavailable", true, true);
  }

  const reasoning = new Map<string, { description?: string; models: string[] }>();
  const serviceTiers = new Map<string, { label: string; description?: string; models: string[] }>();
  for (const model of models) {
    for (const [effort, description] of model.reasoning) {
      const choice = reasoning.get(effort) ?? { description, models: [] };
      choice.models.push(model.id);
      reasoning.set(effort, choice);
    }
    for (const [id, tier] of model.serviceTiers) {
      const choice = serviceTiers.get(id) ?? { ...tier, models: [] };
      choice.models.push(model.id);
      serviceTiers.set(id, choice);
    }
  }

  const controls: RuntimeControlDescriptor[] = [
    accessControl(),
    {
      id: MODEL_CONTROL_ID,
      label: "Model",
      defaultChoiceId: defaultModel.id,
      choices: models.map((model) => ({
        id: model.id,
        label: model.label,
        description: model.description,
        availableWhen: [],
      })),
    },
    {
      id: REASONING_CONTROL_ID,
      label: "Reasoning",
      defaultChoiceId: DEFAULT_REASONING_ID,
      choices: [
        {
          id: DEFAULT_REASONING_ID,
          label: "Model default",
          description: "Use the reasoning level recommended by the selected model",
          availableWhen: [],
        },
        ...[...reasoning].map(([id, choice]) => ({
          id,
          label: labelForReasoning(id),
          description: choice.description,
          availableWhen: [{ controlId: MODEL_CONTROL_ID, choiceIds: choice.models }],
        })),
      ],
    },
    {
      id: SPEED_CONTROL_ID,
      label: "Speed",
      defaultChoiceId: DEFAULT_SPEED_ID,
      choices: [
        {
          id: DEFAULT_SPEED_ID,
          label: "Standard",
          description: "Use the provider's standard service tier",
          availableWhen: [],
        },
        ...[...serviceTiers].map(([id, choice]) => ({
          id,
          label: choice.label,
          description: choice.description,
          availableWhen: [{ controlId: MODEL_CONTROL_ID, choiceIds: choice.models }],
        })),
      ],
    },
  ];

  const modelsById = new Map(models.map((model) => [model.id, model]));
  return {
    controls,
    resolve(selections) {
      const requested = new Map<string, string>();
      for (const selection of selections) {
        if (requested.has(selection.controlId)) {
          throw new RuntimeAdapterError("invalid_request");
        }
        requested.set(selection.controlId, selection.choiceId);
      }
      for (const controlId of requested.keys()) {
        if (!controls.some((control) => control.id === controlId)) {
          throw new RuntimeAdapterError("invalid_request");
        }
      }
      const modelId = requested.get(MODEL_CONTROL_ID) ?? defaultModel.id;
      const model = modelsById.get(modelId);
      if (!model) throw new RuntimeAdapterError("invalid_request");
      const reasoningChoice = requested.get(REASONING_CONTROL_ID) ?? DEFAULT_REASONING_ID;
      const reasoningEffort = reasoningChoice === DEFAULT_REASONING_ID
        ? model.defaultReasoning
        : reasoningChoice;
      if (!model.reasoning.has(reasoningEffort)) {
        throw new RuntimeAdapterError("invalid_request");
      }
      const serviceTierChoice = requested.get(SPEED_CONTROL_ID) ?? DEFAULT_SPEED_ID;
      if (serviceTierChoice !== DEFAULT_SPEED_ID && !model.serviceTiers.has(serviceTierChoice)) {
        throw new RuntimeAdapterError("invalid_request");
      }
      return {
        accessMode: resolveAccessMode(requested),
        model: model.id,
        reasoningEffort,
        serviceTier: serviceTierChoice === DEFAULT_SPEED_ID ? null : serviceTierChoice,
      };
    },
  };
}
