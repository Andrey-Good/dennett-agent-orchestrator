import { constants as fsConstants } from 'node:fs'
import { access, readFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import type { ValidateFunction } from 'ajv'
import type {
	AgentFile,
	AgentNode,
	ArrayParameterDescriptor,
	NumberParameterDescriptor,
	ParameterDescriptor,
	ParameterType,
	StringParameterDescriptor,
} from './agent-file.js'
import type { JsonArray, JsonObject, JsonValue } from './json.js'
import { createAjv2020Validator } from './output-schema-validator.js'

const supportedGraphContractVersions = new Set(['1.0'])
const schemaBundleRelativePath = path.join('contracts', 'json-schema')

let validatorPromise: Promise<ValidateFunction<AgentFile>> | undefined
let schemaDirPromise: Promise<string> | undefined

async function loadJson<T>(filePath: string): Promise<T> {
	return JSON.parse(await readFile(filePath, 'utf8')) as T
}

async function canRead(filePath: string): Promise<boolean> {
	try {
		await access(filePath, fsConstants.R_OK)
		return true
	} catch {
		return false
	}
}

async function resolveSchemaDir(): Promise<string> {
	if (!schemaDirPromise) {
		schemaDirPromise = (async () => {
			let currentDir = path.dirname(fileURLToPath(import.meta.url))

			while (true) {
				const candidateDir = path.join(currentDir, schemaBundleRelativePath)
				const [hasAgentSchema, hasDefsSchema] = await Promise.all([
					canRead(path.join(candidateDir, 'agent-file.schema.json')),
					canRead(path.join(candidateDir, 'agent-json.defs.schema.json')),
				])

				if (hasAgentSchema && hasDefsSchema) {
					return candidateDir
				}

				const parentDir = path.dirname(currentDir)
				if (parentDir === currentDir) {
					throw new Error(
						`Unable to locate contracts/json-schema relative to module ${fileURLToPath(import.meta.url)}.`,
					)
				}
				currentDir = parentDir
			}
		})()
	}

	return schemaDirPromise
}

function rewriteSchemaRefs(value: unknown, fromPrefix: string, toPrefix: string): unknown {
	if (Array.isArray(value)) {
		return value.map((item) => rewriteSchemaRefs(item, fromPrefix, toPrefix))
	}
	if (value && typeof value === 'object') {
		const entries = Object.entries(value as Record<string, unknown>).map(([key, nestedValue]) => {
			if (key === '$ref' && typeof nestedValue === 'string' && nestedValue.startsWith(fromPrefix)) {
				return [key, nestedValue.replace(fromPrefix, toPrefix)] as const
			}
			return [key, rewriteSchemaRefs(nestedValue, fromPrefix, toPrefix)] as const
		})
		return Object.fromEntries(entries)
	}
	return value
}

async function getValidator(): Promise<ValidateFunction<AgentFile>> {
	if (!validatorPromise) {
		validatorPromise = (async () => {
			const schemaDir = await resolveSchemaDir()
			const agentSchemaPath = path.join(schemaDir, 'agent-file.schema.json')
			const defsSchemaPath = path.join(schemaDir, 'agent-json.defs.schema.json')

			const [agentSchema, defsSchema] = await Promise.all([
				loadJson<Record<string, unknown>>(agentSchemaPath),
				loadJson<Record<string, unknown>>(defsSchemaPath),
			])

			const ajv = createAjv2020Validator()

			const agentSchemaId = pathToFileURL(agentSchemaPath).href
			const defsSchemaId = pathToFileURL(defsSchemaPath).href
			agentSchema.$id = agentSchemaId
			defsSchema.$id = defsSchemaId
			const normalizedAgentSchema = rewriteSchemaRefs(
				agentSchema,
				'./agent-json.defs.schema.json#/$defs/',
				'#/$defs/',
			) as Record<string, unknown>
			normalizedAgentSchema.$defs = defsSchema.$defs

			const validator = ajv.compile<AgentFile>(normalizedAgentSchema as never)
			if (!validator) {
				throw new Error('Failed to load agent file schema validator.')
			}

			return validator as ValidateFunction<AgentFile>
		})()
	}

	return validatorPromise
}

function findDuplicates(values: string[]): string[] {
	const seen = new Set<string>()
	const duplicates = new Set<string>()
	for (const value of values) {
		if (seen.has(value)) {
			duplicates.add(value)
		} else {
			seen.add(value)
		}
	}
	return [...duplicates]
}

function assertUniqueIds(values: Array<{ id: string }>, label: string): void {
	const duplicates = findDuplicates(values.map((value) => value.id))
	if (duplicates.length > 0) {
		throw new Error(`${label} contains duplicate ids: ${duplicates.join(', ')}`)
	}
}

function hasValidReferencePath(path: string): boolean {
	return path.length > 0 && !path.startsWith('.') && !path.endsWith('.') && !path.includes('..')
}

function isValidInputReference(reference: string): boolean {
	if (reference.startsWith('params.')) {
		return hasValidReferencePath(reference.slice('params.'.length))
	}

	if (reference.startsWith('vars.')) {
		return hasValidReferencePath(reference.slice('vars.'.length))
	}

	if (reference.startsWith('event.')) {
		return hasValidReferencePath(reference.slice('event.'.length))
	}

	if (!reference.startsWith('node.')) {
		return false
	}

	const segments = reference.split('.')
	if (segments.length < 3 || segments[1]?.length === 0) {
		return false
	}

	if (segments[2] === 'text') {
		return segments.length === 3
	}

	if (segments[2] === 'json') {
		return segments.length === 3 || hasValidReferencePath(segments.slice(3).join('.'))
	}

	return false
}

function assertInputReferences(node: AgentNode): void {
	for (const part of node.input.parts) {
		if (part.type !== 'ref') {
			continue
		}

		if (!isValidInputReference(part.ref)) {
			throw new Error(`Node "${node.id}" contains unsupported input reference "${part.ref}".`)
		}
	}
}

function assertInteractionPolicy(agentFile: AgentFile): void {
	const nodeLookup = new Map(agentFile.nodes.map((node) => [node.id, node] as const))

	const comments = agentFile.interaction?.comments
	if (comments?.enabled) {
		const targetNodeIds = comments.target_node_ids ?? []
		if (targetNodeIds.length === 0) {
			throw new Error('interaction.comments.enabled requires at least one target_node_id.')
		}

		for (const targetNodeId of targetNodeIds) {
			const node = nodeLookup.get(targetNodeId)
			if (!node) {
				throw new Error(
					`interaction.comments.target_node_ids contains unknown node "${targetNodeId}".`,
				)
			}

			if (node.kind !== 'runtime_agent') {
				throw new Error(
					`interaction.comments.target_node_ids contains node "${targetNodeId}" with kind "${node.kind}", but comments may target only runtime_agent nodes.`,
				)
			}
		}
	}

	const userMcp = agentFile.interaction?.user_mcp
	if (userMcp?.server_name !== undefined && userMcp.server_name !== 'orchestrator.user_chat') {
		throw new Error(
			`interaction.user_mcp.server_name must equal "orchestrator.user_chat" when present.`,
		)
	}
}

function assertSecretMarkers(agentFile: AgentFile): void {
	const secretMarkers = agentFile.chat?.secret_markers
	if (
		secretMarkers?.enabled &&
		secretMarkers.open_marker !== undefined &&
		secretMarkers.close_marker !== undefined &&
		secretMarkers.open_marker === secretMarkers.close_marker
	) {
		throw new Error('chat.secret_markers.open_marker and close_marker must be distinct.')
	}
}

function assertRuntimeSourceBindings(agentFile: AgentFile): void {
	const runtimeSourceLookup = new Map(
		(agentFile.runtime_sources ?? []).map(
			(runtimeSource) => [runtimeSource.id, runtimeSource] as const,
		),
	)

	for (const node of agentFile.nodes) {
		if (node.kind !== 'runtime_agent' || !node.runtime_source_ids) {
			continue
		}

		for (const runtimeSourceId of node.runtime_source_ids) {
			const runtimeSource = runtimeSourceLookup.get(runtimeSourceId)
			if (!runtimeSource) {
				throw new Error(`Node "${node.id}" references unknown runtime_source "${runtimeSourceId}".`)
			}

			if (runtimeSource.runtime_adapter !== node.runtime_adapter) {
				throw new Error(
					`Runtime source "${runtimeSourceId}" uses adapter "${runtimeSource.runtime_adapter}" but node "${node.id}" uses "${node.runtime_adapter}".`,
				)
			}
		}
	}
}

function assertMemoryBindings(agentFile: AgentFile): void {
	const memoryBindingLookup = new Map(
		(agentFile.memory_bindings ?? []).map(
			(memoryBinding) => [memoryBinding.id, memoryBinding] as const,
		),
	)

	for (const node of agentFile.nodes) {
		if (node.kind !== 'runtime_agent' || !node.memory_ids) {
			continue
		}

		for (const memoryId of node.memory_ids) {
			if (!memoryBindingLookup.has(memoryId)) {
				throw new Error(`Node "${node.id}" references unknown memory_binding "${memoryId}".`)
			}
		}
	}
}

function getJsonValueType(value: JsonValue): ParameterType {
	if (value === null) {
		return 'null'
	}
	if (Array.isArray(value)) {
		return 'array'
	}
	switch (typeof value) {
		case 'string':
			return 'string'
		case 'number':
			return 'number'
		case 'boolean':
			return 'boolean'
		case 'object':
			return 'object'
		default:
			throw new Error(`Unsupported JSON value type "${typeof value}".`)
	}
}

function sortJsonValue(value: JsonValue): JsonValue {
	if (Array.isArray(value)) {
		return value.map((item) => sortJsonValue(item)) as JsonArray
	}

	if (value && typeof value === 'object') {
		const entries = Object.entries(value)
			.sort(([left], [right]) => left.localeCompare(right))
			.map(([key, nestedValue]) => [key, sortJsonValue(nestedValue)] as const)
		return Object.fromEntries(entries) as JsonObject
	}

	return value
}

function stringifyJsonValue(value: JsonValue): string {
	return JSON.stringify(sortJsonValue(value))
}

function assertJsonValueMatchesType(
	value: JsonValue,
	expectedType: ParameterType,
	label: string,
): void {
	const actualType = getJsonValueType(value)
	if (actualType !== expectedType) {
		throw new Error(`${label} must match declared type "${expectedType}", got "${actualType}".`)
	}
}

function assertStringConstraintValue(
	parameterName: string,
	label: string,
	value: string,
	descriptor: StringParameterDescriptor,
): void {
	const constraints = descriptor.constraints
	if (!constraints) {
		return
	}

	if (constraints.min_length !== undefined && value.length < constraints.min_length) {
		throw new Error(
			`Parameter "${parameterName}" ${label} must have length >= ${constraints.min_length}.`,
		)
	}

	if (constraints.max_length !== undefined && value.length > constraints.max_length) {
		throw new Error(
			`Parameter "${parameterName}" ${label} must have length <= ${constraints.max_length}.`,
		)
	}

	if (constraints.pattern !== undefined && !new RegExp(constraints.pattern, 'u').test(value)) {
		throw new Error(
			`Parameter "${parameterName}" ${label} must match pattern "${constraints.pattern}".`,
		)
	}
}

function assertNumberConstraintValue(
	parameterName: string,
	label: string,
	value: number,
	descriptor: NumberParameterDescriptor,
): void {
	const constraints = descriptor.constraints
	if (!constraints) {
		return
	}

	if (constraints.minimum !== undefined && value < constraints.minimum) {
		throw new Error(`Parameter "${parameterName}" ${label} must be >= ${constraints.minimum}.`)
	}

	if (constraints.maximum !== undefined && value > constraints.maximum) {
		throw new Error(`Parameter "${parameterName}" ${label} must be <= ${constraints.maximum}.`)
	}
}

function assertArrayConstraintValue(
	parameterName: string,
	label: string,
	value: JsonArray,
	descriptor: ArrayParameterDescriptor,
): void {
	const constraints = descriptor.constraints
	if (!constraints) {
		return
	}

	if (constraints.min_items !== undefined && value.length < constraints.min_items) {
		throw new Error(
			`Parameter "${parameterName}" ${label} must contain at least ${constraints.min_items} items.`,
		)
	}

	if (constraints.max_items !== undefined && value.length > constraints.max_items) {
		throw new Error(
			`Parameter "${parameterName}" ${label} must contain at most ${constraints.max_items} items.`,
		)
	}
}

function assertParameterConstraints(parameterName: string, descriptor: ParameterDescriptor): void {
	switch (descriptor.type) {
		case 'string': {
			const constraints = descriptor.constraints
			if (!constraints) {
				return
			}

			if (
				constraints.min_length !== undefined &&
				constraints.max_length !== undefined &&
				constraints.min_length > constraints.max_length
			) {
				throw new Error(`Parameter "${parameterName}" has min_length greater than max_length.`)
			}

			if (constraints.pattern !== undefined) {
				try {
					new RegExp(constraints.pattern, 'u')
				} catch {
					throw new Error(`Parameter "${parameterName}" has an invalid string constraint pattern.`)
				}
			}

			if (descriptor.default !== undefined) {
				assertStringConstraintValue(parameterName, 'default', descriptor.default, descriptor)
			}

			descriptor.allowed_values?.forEach((value, index) => {
				assertStringConstraintValue(parameterName, `allowed_values[${index}]`, value, descriptor)
			})
			return
		}
		case 'number': {
			const constraints = descriptor.constraints
			if (!constraints) {
				return
			}

			if (
				constraints.minimum !== undefined &&
				constraints.maximum !== undefined &&
				constraints.minimum > constraints.maximum
			) {
				throw new Error(`Parameter "${parameterName}" has minimum greater than maximum.`)
			}

			if (descriptor.default !== undefined) {
				assertNumberConstraintValue(parameterName, 'default', descriptor.default, descriptor)
			}

			descriptor.allowed_values?.forEach((value, index) => {
				assertNumberConstraintValue(parameterName, `allowed_values[${index}]`, value, descriptor)
			})
			return
		}
		case 'array': {
			const constraints = descriptor.constraints
			if (!constraints) {
				return
			}

			if (
				constraints.min_items !== undefined &&
				constraints.max_items !== undefined &&
				constraints.min_items > constraints.max_items
			) {
				throw new Error(`Parameter "${parameterName}" has min_items greater than max_items.`)
			}

			if (descriptor.default !== undefined) {
				assertArrayConstraintValue(parameterName, 'default', descriptor.default, descriptor)
			}

			descriptor.allowed_values?.forEach((value, index) => {
				assertArrayConstraintValue(parameterName, `allowed_values[${index}]`, value, descriptor)
			})
			return
		}
		default:
			return
	}
}

function assertParameters(agentFile: AgentFile): void {
	if (!agentFile.params) {
		return
	}

	for (const [parameterName, descriptor] of Object.entries(agentFile.params)) {
		if (descriptor.default !== undefined) {
			assertJsonValueMatchesType(
				descriptor.default,
				descriptor.type,
				`Parameter "${parameterName}" default`,
			)
		}

		const allowedValues = descriptor.allowed_values ?? []
		if (allowedValues.length > 0) {
			const seenValues = new Set<string>()

			allowedValues.forEach((value, index) => {
				assertJsonValueMatchesType(
					value,
					descriptor.type,
					`Parameter "${parameterName}" allowed_values[${index}]`,
				)

				const serializedValue = stringifyJsonValue(value)
				if (seenValues.has(serializedValue)) {
					throw new Error(`Parameter "${parameterName}" allowed_values must be unique.`)
				}
				seenValues.add(serializedValue)
			})

			if (
				descriptor.default !== undefined &&
				!allowedValues.some(
					(value) =>
						stringifyJsonValue(value) === stringifyJsonValue(descriptor.default as JsonValue),
				)
			) {
				throw new Error(
					`Parameter "${parameterName}" default must appear in allowed_values when allowed_values is present.`,
				)
			}
		}

		assertParameterConstraints(parameterName, descriptor)
	}
}

function assertPortableContract(agentFile: AgentFile): void {
	if (!supportedGraphContractVersions.has(agentFile.graph_contract_version)) {
		throw new Error(
			`Unsupported graph_contract_version "${agentFile.graph_contract_version}". Supported: ${[
				...supportedGraphContractVersions,
			].join(', ')}`,
		)
	}

	assertUniqueIds(agentFile.nodes, 'nodes')
	if (agentFile.edges) {
		for (const edge of agentFile.edges) {
			if (!agentFile.nodes.some((node) => node.id === edge.from)) {
				throw new Error(`Edge from "${edge.from}" does not resolve to an existing node.`)
			}
			if (!agentFile.nodes.some((node) => node.id === edge.to)) {
				throw new Error(`Edge to "${edge.to}" does not resolve to an existing node.`)
			}
		}
	}

	if (!agentFile.nodes.some((node) => node.id === agentFile.entry_node_id)) {
		throw new Error(`entry_node_id "${agentFile.entry_node_id}" does not resolve to a node.`)
	}

	assertUniqueIds(agentFile.skills ?? [], 'skills')
	assertUniqueIds(agentFile.mcps ?? [], 'mcps')
	assertUniqueIds(agentFile.plugins ?? [], 'plugins')
	assertUniqueIds(agentFile.memory_bindings ?? [], 'memory_bindings')
	assertUniqueIds(agentFile.runtime_sources ?? [], 'runtime_sources')

	for (const node of agentFile.nodes) {
		assertInputReferences(node)
	}

	if (agentFile.interaction) {
		assertInteractionPolicy(agentFile)
	}

	assertSecretMarkers(agentFile)
	assertMemoryBindings(agentFile)
	assertRuntimeSourceBindings(agentFile)
	assertParameters(agentFile)
}

export async function validateAgentFileValue(raw: unknown): Promise<AgentFile> {
	const validator = await getValidator()

	if (!validator(raw)) {
		const messages = validator.errors
			?.map((error) => `${error.instancePath || '/'} ${error.message}`)
			.join('; ')
		throw new Error(
			messages
				? `Agent file schema validation failed: ${messages}`
				: 'Agent file schema validation failed.',
		)
	}

	assertPortableContract(raw)
	return raw
}

export async function loadAndValidateAgentFile(filePath: string): Promise<AgentFile> {
	const raw = await loadJson<unknown>(filePath)
	return await validateAgentFileValue(raw)
}
