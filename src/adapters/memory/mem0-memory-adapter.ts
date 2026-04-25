import { spawn } from 'node:child_process'
import { existsSync } from 'node:fs'
import type { JsonObject, JsonValue } from '../../core/json.js'
import {
	assertMemoryNegotiation,
	type MemoryAdapter,
	type MemoryCapability,
	type MemoryCapabilityRequirement,
	type MemoryCleanupPreviewRequest,
	type MemoryCleanupPreviewResult,
	MemoryConfigurationError,
	type MemoryDeleteRequest,
	type MemoryDeleteResult,
	MemoryExecutionError,
	type MemoryListRequest,
	type MemoryProviderDescriptor,
	type MemoryReadRequest,
	type MemoryRecord,
	type MemoryScope,
	type MemorySearchRequest,
	type MemorySearchResult,
	type MemoryUpdateRequest,
	type MemoryVerifiedCleanupRequest,
	type MemoryVerifiedCleanupResult,
	type MemoryWriteRequest,
	type MemoryWriteResult,
	negotiateMemoryProviderSupport,
} from '../../ports/memory.js'

const MEM0_BRIDGE_PROGRAM = `
import json
import sys
import traceback

from mem0 import Memory


def emit(payload):
    sys.stdout.write(json.dumps(payload))
    sys.stdout.flush()


def fail(exc):
    emit(
        {
            "ok": False,
            "error": {
                "type": exc.__class__.__name__,
                "message": str(exc),
                "traceback": traceback.format_exc(),
            },
        }
    )


def main():
    request = json.load(sys.stdin)
    config = request["config"]
    action = request["action"]

    memory = Memory.from_config(config)

    try:
        if action == "write":
            result = memory.add(
                request["content"],
                user_id=request.get("user_id"),
                agent_id=request.get("agent_id"),
                run_id=request.get("run_id"),
                metadata=request.get("metadata"),
                infer=request.get("infer", False),
            )
            emit({"ok": True, "result": result})
            return

        if action == "read":
            emit({"ok": True, "result": memory.get(request["memory_id"])})
            return

        if action == "search":
            result = memory.search(
                request["query"],
                filters=request["filters"],
                top_k=request.get("top_k", 20),
                threshold=request.get("threshold", 0.1),
            )
            emit({"ok": True, "result": result})
            return

        if action == "list":
            result = memory.get_all(
                filters=request["filters"],
                top_k=request.get("top_k", 20),
            )
            emit({"ok": True, "result": result})
            return

        if action == "update":
            update_result = memory.update(
                request["memory_id"],
                request["content"],
                metadata=request.get("metadata"),
            )
            read_back = memory.get(request["memory_id"])
            emit({"ok": True, "result": {"update": update_result, "record": read_back}})
            return

        if action == "delete":
            result = memory.delete(request["memory_id"])
            emit({"ok": True, "result": result})
            return

        raise ValueError(f"Unsupported Mem0 bridge action: {action}")
    except Exception as exc:
        fail(exc)
    finally:
        close_fn = getattr(memory, "close", None)
        if callable(close_fn):
            close_fn()


if __name__ == "__main__":
    main()
`

interface Mem0BridgeSuccess {
	ok: true
	result: JsonValue
}

interface Mem0BridgeFailure {
	ok: false
	error: {
		type?: string
		message?: string
		traceback?: string
	}
}

type Mem0BridgeResponse = Mem0BridgeSuccess | Mem0BridgeFailure

export interface Mem0BridgeRunnerContext {
	python_executable: string
	working_directory?: string
	bridge_timeout_ms: number
	bridge_program: string
}

export interface Mem0BridgeRunnerResult {
	exit_code: number | null
	stdout: string
	stderr: string
}

export type Mem0BridgeRunner = (
	request: JsonObject,
	context: Mem0BridgeRunnerContext,
) => Promise<Mem0BridgeRunnerResult>

const DENNETT_NAMESPACE_METADATA_KEY = 'dennett_namespace_id'
const DEFAULT_CLEANUP_LIMIT = 10000

export interface Mem0MemoryAdapterConfig {
	python_executable: string
	mem0_config: JsonObject
	working_directory?: string
	bridge_timeout_ms?: number
	bridge_runner?: Mem0BridgeRunner
}

function isObject(value: JsonValue | unknown): value is JsonObject {
	return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function normalizeScope(scope: MemoryScope): MemoryScope {
	const normalized: MemoryScope = {}
	if (typeof scope.user_id === 'string' && scope.user_id.trim().length > 0) {
		normalized.user_id = scope.user_id.trim()
	}
	if (typeof scope.agent_id === 'string' && scope.agent_id.trim().length > 0) {
		normalized.agent_id = scope.agent_id.trim()
	}
	if (typeof scope.run_id === 'string' && scope.run_id.trim().length > 0) {
		normalized.run_id = scope.run_id.trim()
	}

	if (!normalized.user_id && !normalized.agent_id && !normalized.run_id) {
		throw new MemoryConfigurationError(
			'At least one of user_id, agent_id, or run_id must be provided for memory operations.',
		)
	}

	return normalized
}

function applyScopeToJsonObject(target: JsonObject, scope: MemoryScope): void {
	if (scope.user_id !== undefined) {
		target.user_id = scope.user_id
	}
	if (scope.agent_id !== undefined) {
		target.agent_id = scope.agent_id
	}
	if (scope.run_id !== undefined) {
		target.run_id = scope.run_id
	}
}

function extractNamespaceId(config: JsonObject): string | undefined {
	const namespaceId = config[DENNETT_NAMESPACE_METADATA_KEY]
	if (typeof namespaceId !== 'string') {
		return undefined
	}

	const trimmed = namespaceId.trim()
	return trimmed.length > 0 ? trimmed : undefined
}

function cloneJsonObject(value: JsonObject | undefined): JsonObject {
	return value ? { ...value } : {}
}

function buildMetadataWithNamespace(
	metadata: JsonObject | undefined,
	namespaceId: string | undefined,
): JsonObject | undefined {
	if (!namespaceId) {
		return metadata
	}

	return {
		...cloneJsonObject(metadata),
		[DENNETT_NAMESPACE_METADATA_KEY]: namespaceId,
	}
}

function applyNamespaceToFilters(filters: JsonObject, namespaceId: string | undefined): JsonObject {
	if (!namespaceId) {
		return filters
	}

	return {
		...filters,
		[DENNETT_NAMESPACE_METADATA_KEY]: namespaceId,
	}
}

function recordMatchesNamespace(record: MemoryRecord, namespaceId: string | undefined): boolean {
	if (!namespaceId) {
		return true
	}

	return record.metadata?.[DENNETT_NAMESPACE_METADATA_KEY] === namespaceId
}

function recordMatchesScope(record: MemoryRecord, scope: MemoryScope): boolean {
	return (
		(scope.user_id === undefined || record.scope.user_id === scope.user_id) &&
		(scope.agent_id === undefined || record.scope.agent_id === scope.agent_id) &&
		(scope.run_id === undefined || record.scope.run_id === scope.run_id)
	)
}

function resolveCleanupLimit(limit: number | undefined): number {
	const resolvedLimit = limit ?? DEFAULT_CLEANUP_LIMIT
	if (
		!Number.isSafeInteger(resolvedLimit) ||
		resolvedLimit <= 0 ||
		resolvedLimit >= Number.MAX_SAFE_INTEGER
	) {
		throw new MemoryConfigurationError('Memory cleanup limit must be a positive safe integer.', {
			limit: limit ?? null,
		})
	}

	return resolvedLimit
}

function toProviderData(source: JsonObject, consumedKeys: Set<string>): JsonObject | undefined {
	const providerData: JsonObject = {}
	for (const [key, value] of Object.entries(source)) {
		if (!consumedKeys.has(key)) {
			providerData[key] = value
		}
	}
	return Object.keys(providerData).length > 0 ? providerData : undefined
}

function normalizeMem0Record(
	value: JsonValue | null,
	fallbackScope?: MemoryScope,
): MemoryRecord | null {
	if (!isObject(value)) {
		return null
	}
	if (typeof value.id !== 'string' || typeof value.memory !== 'string') {
		return null
	}

	const consumedKeys = new Set([
		'id',
		'memory',
		'metadata',
		'score',
		'created_at',
		'updated_at',
		'user_id',
		'agent_id',
		'run_id',
	])

	return {
		id: value.id,
		content: value.memory,
		scope: {
			user_id: typeof value.user_id === 'string' ? value.user_id : fallbackScope?.user_id,
			agent_id: typeof value.agent_id === 'string' ? value.agent_id : fallbackScope?.agent_id,
			run_id: typeof value.run_id === 'string' ? value.run_id : fallbackScope?.run_id,
		},
		metadata: isObject(value.metadata) ? value.metadata : undefined,
		score: typeof value.score === 'number' ? value.score : undefined,
		created_at: typeof value.created_at === 'string' ? value.created_at : undefined,
		updated_at: typeof value.updated_at === 'string' ? value.updated_at : undefined,
		provider_data: toProviderData(value, consumedKeys),
	}
}

function normalizeMem0Results(
	value: JsonValue | null,
	fallbackScope?: MemoryScope,
): MemoryRecord[] {
	if (!isObject(value)) {
		return []
	}

	const results = value.results
	if (!Array.isArray(results)) {
		return []
	}

	return results
		.map((result) => normalizeMem0Record(isObject(result) ? result : null, fallbackScope))
		.filter((record): record is MemoryRecord => record !== null)
}

function deriveSupportedCapabilities(config: JsonObject): MemoryCapability[] {
	const capabilities: MemoryCapability[] = [
		'read',
		'write',
		'entity_scoped',
		'user_scoped',
		'session_scoped',
		'infer_extract',
	]

	if (
		isObject(config.graph_store) &&
		typeof config.graph_store.provider === 'string' &&
		config.graph_store.provider.trim().length > 0
	) {
		capabilities.push('graph_context')
	}

	return capabilities
}

function jsonErrorDetails(details: Record<string, unknown>): JsonObject {
	return details as JsonObject
}

function validateBridgeResponse(response: unknown): Mem0BridgeResponse {
	if (!isObject(response) || typeof response.ok !== 'boolean') {
		throw new MemoryExecutionError('Mem0 python bridge returned an unsupported response.', {
			response_shape: isObject(response) ? Object.keys(response).sort() : typeof response,
		})
	}

	if (response.ok) {
		if (!('result' in response)) {
			throw new MemoryExecutionError(
				'Mem0 python bridge returned a success response without result.',
				{
					response_shape: Object.keys(response).sort(),
				},
			)
		}

		return {
			ok: true,
			result: response.result,
		}
	}

	if (!isObject(response.error)) {
		throw new MemoryExecutionError(
			'Mem0 python bridge returned a failure response without error.',
			{
				response_shape: Object.keys(response).sort(),
			},
		)
	}

	return {
		ok: false,
		error: {
			type: typeof response.error.type === 'string' ? response.error.type : undefined,
			message: typeof response.error.message === 'string' ? response.error.message : undefined,
			traceback:
				typeof response.error.traceback === 'string' ? response.error.traceback : undefined,
		},
	}
}

function isMem0MissingRecordError(error: unknown): boolean {
	if (!(error instanceof MemoryExecutionError)) {
		return false
	}
	if (!isObject(error.details)) {
		return false
	}

	const errorType = error.details.error_type
	const traceback = error.details.traceback
	return (
		errorType === 'IndexError' ||
		(typeof traceback === 'string' &&
			(traceback.includes('IndexError') || traceback.includes('not found')))
	)
}

async function runMem0BridgeProcess(
	request: JsonObject,
	context: Mem0BridgeRunnerContext,
): Promise<Mem0BridgeRunnerResult> {
	return new Promise<Mem0BridgeRunnerResult>((resolve, reject) => {
		const child = spawn(context.python_executable, ['-c', context.bridge_program], {
			cwd: context.working_directory,
			stdio: ['pipe', 'pipe', 'pipe'],
			windowsHide: true,
		})

		let stdout = ''
		let stderr = ''
		let settled = false

		const settle = (callback: () => void) => {
			if (settled) {
				return
			}
			settled = true
			clearTimeout(timeoutHandle)
			callback()
		}

		const timeoutHandle = setTimeout(() => {
			if (!child.killed) {
				child.kill()
			}
			settle(() => {
				reject(
					new MemoryExecutionError('Mem0 python bridge timed out.', {
						action: request.action ?? null,
						timeout_ms: context.bridge_timeout_ms,
						python_executable: context.python_executable,
						stdout,
						stderr,
					}),
				)
			})
		}, context.bridge_timeout_ms)

		child.stdout.on('data', (chunk: Buffer) => {
			stdout += chunk.toString('utf8')
		})
		child.stderr.on('data', (chunk: Buffer) => {
			stderr += chunk.toString('utf8')
		})
		child.once('error', (error) => {
			settle(() => {
				reject(
					new MemoryExecutionError('Failed to launch the Mem0 python bridge.', {
						message: error.message,
						python_executable: context.python_executable,
					}),
				)
			})
		})
		child.once('close', (code) => {
			settle(() => {
				resolve({
					exit_code: code,
					stdout,
					stderr,
				})
			})
		})

		child.stdin.end(JSON.stringify(request))
	})
}

async function runBridgeRunnerWithTimeout(
	bridgeRunner: Mem0BridgeRunner,
	request: JsonObject,
	context: Mem0BridgeRunnerContext,
): Promise<Mem0BridgeRunnerResult> {
	let timeoutHandle: ReturnType<typeof setTimeout> | undefined

	const timeoutPromise = new Promise<never>((_, reject) => {
		timeoutHandle = setTimeout(() => {
			reject(
				new MemoryExecutionError('Mem0 python bridge timed out.', {
					action: request.action ?? null,
					timeout_ms: context.bridge_timeout_ms,
					python_executable: context.python_executable,
					stdout: '',
					stderr: '',
				}),
			)
		}, context.bridge_timeout_ms)
	})

	try {
		return await Promise.race([bridgeRunner(request, context), timeoutPromise])
	} finally {
		if (timeoutHandle !== undefined) {
			clearTimeout(timeoutHandle)
		}
	}
}

export class Mem0MemoryAdapter implements MemoryAdapter {
	private readonly descriptor: MemoryProviderDescriptor
	private readonly namespaceId: string | undefined

	constructor(private readonly config: Mem0MemoryAdapterConfig) {
		if (!existsSync(config.python_executable)) {
			throw new MemoryConfigurationError('Configured Mem0 python executable does not exist.', {
				python_executable: config.python_executable,
			})
		}

		this.namespaceId = extractNamespaceId(config.mem0_config)
		this.descriptor = {
			provider: 'mem0',
			display_name: 'Mem0',
			supported_capabilities: deriveSupportedCapabilities(config.mem0_config),
			supported_transports: ['sdk'],
			default_transport: 'sdk',
		}
	}

	describeProvider(): MemoryProviderDescriptor {
		return this.descriptor
	}

	negotiate(requirement: MemoryCapabilityRequirement) {
		return negotiateMemoryProviderSupport(this.descriptor, requirement)
	}

	async writeMemory(request: MemoryWriteRequest): Promise<MemoryWriteResult> {
		this.ensureCapabilities({
			required_capabilities: ['write'],
		})

		const scope = normalizeScope(request.scope)
		const bridgePayload: JsonObject = {
			action: 'write',
			content: request.content,
			infer: request.infer ?? false,
		}
		applyScopeToJsonObject(bridgePayload, scope)
		const metadata = buildMetadataWithNamespace(request.metadata, this.namespaceId)
		if (metadata) {
			bridgePayload.metadata = metadata
		}

		const result = await this.runBridge(bridgePayload)

		return {
			records: normalizeMem0Results(result, scope),
		}
	}

	async readMemory(request: MemoryReadRequest): Promise<MemoryRecord | null> {
		this.ensureCapabilities({
			required_capabilities: ['read'],
		})

		try {
			const result = await this.runBridge({
				action: 'read',
				memory_id: request.memory_id,
			})
			const record = normalizeMem0Record(result)
			if (!record || !recordMatchesNamespace(record, this.namespaceId)) {
				return null
			}
			return record
		} catch (error) {
			if (isMem0MissingRecordError(error)) {
				return null
			}
			throw error
		}
	}

	async searchMemory(request: MemorySearchRequest): Promise<MemorySearchResult> {
		this.ensureCapabilities({
			required_capabilities: ['read'],
		})

		const scope = normalizeScope(request.scope)
		const filters = applyNamespaceToFilters(scope as JsonObject, this.namespaceId)
		const result = await this.runBridge({
			action: 'search',
			query: request.query,
			filters,
			top_k: request.limit ?? 20,
			threshold: request.threshold ?? 0.1,
		})

		return {
			records: normalizeMem0Results(result).filter((record) =>
				recordMatchesNamespace(record, this.namespaceId),
			),
		}
	}

	async listMemories(request: MemoryListRequest): Promise<MemoryRecord[]> {
		this.ensureCapabilities({
			required_capabilities: ['read'],
		})

		const scope = normalizeScope(request.scope)
		const filters = applyNamespaceToFilters(scope as JsonObject, this.namespaceId)
		const result = await this.runBridge({
			action: 'list',
			filters,
			top_k: request.limit ?? 20,
		})

		return normalizeMem0Results(result).filter((record) =>
			recordMatchesNamespace(record, this.namespaceId),
		)
	}

	async updateMemory(request: MemoryUpdateRequest): Promise<MemoryRecord | null> {
		this.ensureCapabilities({
			required_capabilities: ['write', 'read'],
		})

		if (this.namespaceId) {
			const existingRecord = await this.readMemory({
				memory_id: request.memory_id,
			})
			if (!existingRecord) {
				return null
			}
		}

		const bridgePayload: JsonObject = {
			action: 'update',
			memory_id: request.memory_id,
			content: request.content,
		}
		const metadata = buildMetadataWithNamespace(request.metadata, this.namespaceId)
		if (metadata) {
			bridgePayload.metadata = metadata
		}

		const result = await this.runBridge(bridgePayload)

		if (!isObject(result) || !('record' in result)) {
			return null
		}

		const record = normalizeMem0Record(result.record as JsonValue)
		if (!record || !recordMatchesNamespace(record, this.namespaceId)) {
			return null
		}
		return record
	}

	async deleteMemory(request: MemoryDeleteRequest): Promise<MemoryDeleteResult> {
		this.ensureCapabilities({
			required_capabilities: ['write'],
		})

		if (this.namespaceId) {
			const existingRecord = await this.readMemory({
				memory_id: request.memory_id,
			})
			if (!existingRecord) {
				return {
					deleted: false,
				}
			}
		}

		await this.runBridge({
			action: 'delete',
			memory_id: request.memory_id,
		})

		return {
			deleted: true,
		}
	}

	async previewMemoryCleanup(
		request: MemoryCleanupPreviewRequest,
	): Promise<MemoryCleanupPreviewResult> {
		const namespaceId = this.requireNamespaceId()
		const limit = resolveCleanupLimit(request.limit)

		const records = await this.listMemories({
			scope: request.scope,
			limit: limit + 1,
		})
		const candidateIds = records.slice(0, limit).map((record) => record.id)

		return {
			namespace_id: namespaceId,
			candidate_ids: candidateIds,
			candidate_count: candidateIds.length,
			limit,
			truncated: records.length > limit,
		}
	}

	async deleteMemoryCleanup(
		request: MemoryVerifiedCleanupRequest,
	): Promise<MemoryVerifiedCleanupResult> {
		const namespaceId = this.requireNamespaceId()
		const limit = resolveCleanupLimit(request.limit)
		const scope = normalizeScope(request.scope)

		let requestedIds: string[]
		let candidateIds: string[]
		let requestedTruncated = false
		const skippedIds: string[] = []

		if (request.candidate_ids !== undefined) {
			requestedIds = [...request.candidate_ids]
			candidateIds = []

			for (const candidateId of [...new Set(request.candidate_ids)]) {
				const record = await this.readMemory({
					memory_id: candidateId,
				})
				if (record && recordMatchesScope(record, scope)) {
					candidateIds.push(candidateId)
				} else {
					skippedIds.push(candidateId)
				}
			}
		} else {
			const preview = await this.previewMemoryCleanup({
				scope,
				limit,
			})
			requestedIds = preview.candidate_ids
			candidateIds = preview.candidate_ids
			requestedTruncated = preview.truncated
		}

		const deletedIds: string[] = []
		for (const candidateId of candidateIds) {
			const result = await this.deleteMemory({
				memory_id: candidateId,
			})
			if (result.deleted) {
				deletedIds.push(candidateId)
			}
		}

		const verification = await this.previewMemoryCleanup({
			scope,
			limit,
		})

		return {
			namespace_id: namespaceId,
			limit,
			requested_ids: requestedIds,
			deleted_ids: deletedIds,
			skipped_ids: skippedIds,
			remaining_ids: verification.candidate_ids,
			requested_truncated: requestedTruncated,
			remaining_truncated: verification.truncated,
			verified_empty: !verification.truncated && verification.candidate_count === 0,
		}
	}

	private ensureCapabilities(requirement: MemoryCapabilityRequirement): void {
		assertMemoryNegotiation(this.descriptor, requirement)
	}

	private requireNamespaceId(): string {
		if (!this.namespaceId) {
			throw new MemoryConfigurationError(
				'Mem0 namespace cleanup requires mem0_config.dennett_namespace_id.',
			)
		}

		return this.namespaceId
	}

	private async runBridge(payload: JsonObject): Promise<JsonValue | null> {
		const request = {
			action: payload.action,
			config: this.config.mem0_config,
			...payload,
		}
		const bridgeTimeoutMs = this.config.bridge_timeout_ms ?? 120000
		const bridgeRunner = this.config.bridge_runner ?? runMem0BridgeProcess
		const bridgeResult = await runBridgeRunnerWithTimeout(bridgeRunner, request, {
			python_executable: this.config.python_executable,
			working_directory: this.config.working_directory,
			bridge_timeout_ms: bridgeTimeoutMs,
			bridge_program: MEM0_BRIDGE_PROGRAM,
		})

		if (bridgeResult.exit_code !== 0) {
			throw new MemoryExecutionError('Mem0 python bridge exited unsuccessfully.', {
				exit_code: bridgeResult.exit_code,
				stderr: bridgeResult.stderr,
				stdout: bridgeResult.stdout,
			})
		}

		if (bridgeResult.stdout.trim().length === 0) {
			throw new MemoryExecutionError('Mem0 python bridge returned an empty response.', {
				stderr: bridgeResult.stderr,
			})
		}

		let parsedResponse: unknown
		try {
			parsedResponse = JSON.parse(bridgeResult.stdout)
		} catch (error) {
			throw new MemoryExecutionError('Mem0 python bridge returned invalid JSON.', {
				stdout: bridgeResult.stdout,
				stderr: bridgeResult.stderr,
				message: error instanceof Error ? error.message : 'Unknown JSON parse error.',
			})
		}

		const response = validateBridgeResponse(parsedResponse)

		if (!response.ok) {
			throw new MemoryExecutionError(
				response.error.message ?? 'Mem0 bridge operation failed.',
				jsonErrorDetails({
					error_type: response.error.type ?? 'UnknownError',
					traceback: response.error.traceback ?? '',
				}),
			)
		}

		return response.result ?? null
	}
}
