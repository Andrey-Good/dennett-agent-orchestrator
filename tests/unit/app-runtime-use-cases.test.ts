import { describe, expect, it, vi } from 'vitest'
import {
	inspectRuntimeEnvironment,
	listRuntimeModels,
} from '../../src/app/runtime-use-cases.js'
import type { AppError } from '../../src/core/errors.js'
import type {
	RuntimeAdapter,
	RuntimeEnvironmentInspectionResult,
	RuntimeModelCatalogPage,
	RuntimeModelCatalogRequest,
} from '../../src/ports/runtime.js'

function createRuntimeSurfaceAdapter(overrides?: {
	models?: RuntimeModelCatalogPage
	environment?: RuntimeEnvironmentInspectionResult
	supportsModelDiscovery?: boolean
	supportsEnvironmentInspection?: boolean
	listModelsError?: Error
	inspectEnvironmentError?: Error
}) {
	const listModels = vi.fn<
		(request?: RuntimeModelCatalogRequest) => Promise<RuntimeModelCatalogPage>
	>(async (request) => {
		if (overrides?.listModelsError) {
			throw overrides.listModelsError
		}
		return {
			models: [],
			...(request?.cursor ? { next_cursor: request.cursor } : {}),
			...(overrides?.models ?? {}),
		}
	})
	const inspectEnvironment = vi.fn<() => Promise<RuntimeEnvironmentInspectionResult>>(async () => {
		if (overrides?.inspectEnvironmentError) {
			throw overrides.inspectEnvironmentError
		}
		return {
			auth: {
				authenticated: false,
				requires_openai_auth: false,
			},
			account: {
				status: 'unknown',
			},
			rate_limits: [],
			config: {},
			...(overrides?.environment ?? {}),
		}
	})

	const adapter: RuntimeAdapter = {
		describeCapabilities() {
			return {
				supports_native_resume: false,
				supports_live_comments: false,
				supports_builtin_user_chat_mcp: false,
				supports_memory_bindings: false,
				supports_model_discovery: overrides?.supportsModelDiscovery ?? true,
				supports_runtime_environment_introspection:
					overrides?.supportsEnvironmentInspection ?? true,
				supports_reasoning_effort: true,
				supports_speed_tiers: true,
				supports_personality: true,
				supports_explicit_runtime_source: false,
				supports_runtime_source_introspection: false,
			}
		},
		async startExecution() {
			throw new Error('not used in runtime app use-case tests')
		},
		listModels,
		inspectRuntimeEnvironment: inspectEnvironment,
		async inspectRuntimeSource() {
			throw new Error('not used in runtime app use-case tests')
		},
		async deliverComment() {
			throw new Error('not used in runtime app use-case tests')
		},
		async deliverUserChatResponse() {
			throw new Error('not used in runtime app use-case tests')
		},
		async cancelExecution() {
			throw new Error('not used in runtime app use-case tests')
		},
	}

	return {
		adapter,
		listModels,
		inspectEnvironment,
	}
}

describe('runtime app use cases', () => {
	it('lists models through the normalized runtime surface', async () => {
		const harness = createRuntimeSurfaceAdapter({
			models: {
				models: [
					{
						id: 'gpt-5.3-codex',
						hidden: false,
						is_default: true,
						input_modalities: ['text'],
						supports_personality: true,
						supported_reasoning_efforts: ['minimal', 'medium'],
						additional_speed_tiers: ['fast'],
					},
				],
				next_cursor: 'cursor-2',
			},
		})

		const result = await listRuntimeModels(
			{
				cursor: 'cursor-1',
				limit: 5,
				includeHidden: true,
			},
			harness.adapter,
		)

		expect(harness.listModels).toHaveBeenCalledWith({
			cursor: 'cursor-1',
			limit: 5,
			include_hidden: true,
		})
		expect(result).toEqual({
			models: [
				{
					id: 'gpt-5.3-codex',
					hidden: false,
					is_default: true,
					input_modalities: ['text'],
					supports_personality: true,
					supported_reasoning_efforts: ['minimal', 'medium'],
					additional_speed_tiers: ['fast'],
				},
			],
			next_cursor: 'cursor-2',
		})
	})

	it('inspects the runtime environment and can redact private diagnostics fields', async () => {
		const harness = createRuntimeSurfaceAdapter({
			environment: {
				auth: {
					authenticated: true,
					auth_method: 'chatgpt',
					requires_openai_auth: false,
				},
				account: {
					status: 'available',
					account_type: 'chatgpt',
					email: 'alice@example.com',
				},
				rate_limits: [],
				config: {
					model: 'gpt-5.3-codex',
					profile: 'C:\\Users\\Alice\\private\\profile.toml',
				},
			},
		})

		const result = await inspectRuntimeEnvironment(harness.adapter, { redacted: true })
		const serialized = JSON.stringify(result)

		expect(harness.inspectEnvironment).toHaveBeenCalledTimes(1)
		expect(serialized).not.toContain('alice@example.com')
		expect(serialized).not.toContain('C:\\Users\\Alice')
		expect(serialized).toContain('[REDACTED_EMAIL]')
		expect(serialized).toContain('[REDACTED_PATH]')
		expect(serialized).toContain('gpt-5.3-codex')
	})

	it('fails before adapter calls when runtime diagnostics capabilities are unsupported', async () => {
		const modelHarness = createRuntimeSurfaceAdapter({
			supportsModelDiscovery: false,
		})
		const environmentHarness = createRuntimeSurfaceAdapter({
			supportsEnvironmentInspection: false,
		})

		await expect(listRuntimeModels({ limit: 5 }, modelHarness.adapter)).rejects.toMatchObject({
			code: 'UNSUPPORTED_RUNTIME_SURFACE',
			message: 'The current runtime adapter does not support model discovery.',
		} satisfies Pick<AppError, 'code' | 'message'>)
		await expect(inspectRuntimeEnvironment(environmentHarness.adapter)).rejects.toMatchObject({
			code: 'UNSUPPORTED_RUNTIME_SURFACE',
			message: 'The current runtime adapter does not support runtime environment introspection.',
		} satisfies Pick<AppError, 'code' | 'message'>)

		expect(modelHarness.listModels).not.toHaveBeenCalled()
		expect(environmentHarness.inspectEnvironment).not.toHaveBeenCalled()
	})

	it('propagates adapter errors after capability checks pass', async () => {
		const failure = new Error('runtime surface unavailable')
		const harness = createRuntimeSurfaceAdapter({
			listModelsError: failure,
		})

		await expect(listRuntimeModels({}, harness.adapter)).rejects.toThrow(
			'runtime surface unavailable',
		)
		expect(harness.listModels).toHaveBeenCalledTimes(1)
	})
})
