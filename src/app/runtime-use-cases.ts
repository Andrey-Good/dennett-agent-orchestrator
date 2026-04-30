import { redactDiagnosticsValue } from '../core/diagnostics-redaction.js'
import { AppError } from '../core/errors.js'
import type { JsonValue } from '../core/json.js'
import type {
	RuntimeAdapter,
	RuntimeEnvironmentInspectionResult,
	RuntimeModelCatalogPage,
} from '../ports/runtime.js'

export interface ListRuntimeModelsInput {
	cursor?: string
	limit?: number
	includeHidden?: boolean
}

export interface InspectRuntimeEnvironmentOptions {
	redacted?: boolean
}

export async function listRuntimeModels(
	input: ListRuntimeModelsInput,
	runtimeAdapter: RuntimeAdapter,
): Promise<RuntimeModelCatalogPage> {
	const capabilities = runtimeAdapter.describeCapabilities()
	if (!capabilities.supports_model_discovery) {
		throw new AppError(
			'UNSUPPORTED_RUNTIME_SURFACE',
			'The current runtime adapter does not support model discovery.',
		)
	}

	return await runtimeAdapter.listModels({
		...(input.cursor ? { cursor: input.cursor } : {}),
		...(input.limit !== undefined ? { limit: input.limit } : {}),
		...(input.includeHidden !== undefined ? { include_hidden: input.includeHidden } : {}),
	})
}

export async function inspectRuntimeEnvironment(
	runtimeAdapter: RuntimeAdapter,
	options: InspectRuntimeEnvironmentOptions = {},
): Promise<RuntimeEnvironmentInspectionResult | JsonValue> {
	const capabilities = runtimeAdapter.describeCapabilities()
	if (!capabilities.supports_runtime_environment_introspection) {
		throw new AppError(
			'UNSUPPORTED_RUNTIME_SURFACE',
			'The current runtime adapter does not support runtime environment introspection.',
		)
	}

	const result = await runtimeAdapter.inspectRuntimeEnvironment()
	return options.redacted ? redactDiagnosticsValue(result as unknown as JsonValue) : result
}
