import { describe, expect, it } from 'vitest'
import {
	type Mem0BridgeRunner,
	Mem0MemoryAdapter,
} from '../../src/adapters/memory/mem0-memory-adapter.js'
import { MemoryExecutionError } from '../../src/ports/memory.js'

function createAdapter(bridgeRunner: Mem0BridgeRunner): Mem0MemoryAdapter {
	return new Mem0MemoryAdapter({
		python_executable: process.execPath,
		mem0_config: {
			version: 'reliability-test',
		},
		bridge_timeout_ms: 5,
		bridge_runner: bridgeRunner,
	})
}

async function write(adapter: Mem0MemoryAdapter) {
	return adapter.writeMemory({
		content: 'reliable bridge coverage',
		scope: {
			user_id: 'mem0-reliability-user',
		},
		infer: false,
	})
}

async function expectWriteExecutionError(
	bridgeRunner: Mem0BridgeRunner,
): Promise<MemoryExecutionError> {
	try {
		await write(createAdapter(bridgeRunner))
		throw new Error('Expected writeMemory to fail.')
	} catch (error) {
		expect(error).toBeInstanceOf(MemoryExecutionError)
		return error as MemoryExecutionError
	}
}

describe('Mem0MemoryAdapter bridge reliability', () => {
	it('enforces bridge_timeout_ms around a non-settling injected bridge runner', async () => {
		const startedAt = Date.now()
		const error = await expectWriteExecutionError(() => new Promise<never>(() => undefined))

		expect(error.message).toBe('Mem0 python bridge timed out.')
		expect(error.details).toEqual({
			action: 'write',
			timeout_ms: 5,
			python_executable: process.execPath,
			stdout: '',
			stderr: '',
		})
		expect(Date.now() - startedAt).toBeLessThan(1000)
	})

	it('surfaces nonzero bridge exits as controlled execution failures', async () => {
		const error = await expectWriteExecutionError(async () => ({
			exit_code: 7,
			stdout: '{"ok":true,"result":{"ignored":true}}',
			stderr: 'provider failed before emitting a usable response',
		}))

		expect(error.message).toBe('Mem0 python bridge exited unsuccessfully.')
		expect(error.details).toEqual({
			exit_code: 7,
			stdout: '{"ok":true,"result":{"ignored":true}}',
			stderr: 'provider failed before emitting a usable response',
		})
	})

	it('allows stderr warnings when the bridge exits successfully with a valid response', async () => {
		const adapter = createAdapter(async () => ({
			exit_code: 0,
			stderr: 'warning: provider emitted a non-fatal advisory',
			stdout: JSON.stringify({
				ok: true,
				result: {
					results: [
						{
							id: 'memory-1',
							memory: 'reliable bridge coverage',
							user_id: 'mem0-reliability-user',
							metadata: {
								source: 'reliability-test',
							},
						},
					],
				},
			}),
		}))

		await expect(write(adapter)).resolves.toEqual({
			records: [
				{
					id: 'memory-1',
					content: 'reliable bridge coverage',
					scope: {
						user_id: 'mem0-reliability-user',
					},
					metadata: {
						source: 'reliability-test',
					},
				},
			],
		})
	})

	it('fails safely when the bridge returns malformed JSON', async () => {
		const error = await expectWriteExecutionError(async () => ({
			exit_code: 0,
			stdout: '{"ok": true,',
			stderr: 'warning before malformed payload',
		}))

		expect(error.message).toBe('Mem0 python bridge returned invalid JSON.')
		expect(error.details).toMatchObject({
			stdout: '{"ok": true,',
			stderr: 'warning before malformed payload',
		})
		expect(error.details.message).toEqual(expect.any(String))
	})

	it('fails safely when the bridge returns an unsupported response shape', async () => {
		const error = await expectWriteExecutionError(async () => ({
			exit_code: 0,
			stdout: JSON.stringify({
				ok: true,
			}),
			stderr: '',
		}))

		expect(error.message).toBe('Mem0 python bridge returned a success response without result.')
		expect(error.details).toEqual({
			response_shape: ['ok'],
		})
	})

	it('normalizes provider-owned bridge failures into execution errors', async () => {
		const error = await expectWriteExecutionError(async () => ({
			exit_code: 0,
			stderr: '',
			stdout: JSON.stringify({
				ok: false,
				error: {
					type: 'ProviderError',
					message: 'provider rejected the request',
					traceback: 'Traceback intentionally owned by provider',
				},
			}),
		}))

		expect(error.message).toBe('provider rejected the request')
		expect(error.details).toEqual({
			error_type: 'ProviderError',
			traceback: 'Traceback intentionally owned by provider',
		})
	})
})
