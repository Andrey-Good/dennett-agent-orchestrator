import type { AgentFile, AgentNode, InputPart, NodeId } from './agent-file.js'
import { AppError } from './errors.js'
import type { JsonObject, JsonValue } from './json.js'

export type NodeOutputRecord = { mode: 'text'; text: string } | { mode: 'json'; json: JsonObject }

export interface RunStateSnapshot {
	params: Record<string, JsonValue>
	vars: Record<string, JsonValue>
	nodeOutputs: Map<NodeId, NodeOutputRecord>
	event?: JsonObject
}

function stringifyInputValue(value: JsonValue): string {
	if (typeof value === 'string') {
		return value
	}
	if (value === null) {
		return 'null'
	}
	if (typeof value === 'number' || typeof value === 'boolean') {
		return String(value)
	}
	return JSON.stringify(value)
}

function resolveJsonPath(root: JsonValue, path: string, sourceRef: string): JsonValue {
	const segments = path.split('.').filter(Boolean)
	let cursor: JsonValue = root

	for (const segment of segments) {
		if (cursor === null || typeof cursor !== 'object') {
			throw new AppError(
				'RESOLUTION_ERROR',
				`Reference "${sourceRef}" does not resolve to an object path.`,
			)
		}

		if (Array.isArray(cursor)) {
			const index = Number(segment)
			if (!Number.isInteger(index) || index < 0 || index >= cursor.length) {
				throw new AppError(
					'RESOLUTION_ERROR',
					`Reference "${sourceRef}" does not resolve to an array element.`,
				)
			}
			cursor = cursor[index] as JsonValue
			continue
		}

		if (!(segment in cursor)) {
			throw new AppError('RESOLUTION_ERROR', `Reference "${sourceRef}" does not exist.`)
		}
		cursor = (cursor as JsonObject)[segment] as JsonValue
	}

	return cursor
}

export function resolveInputReference(reference: string, state: RunStateSnapshot): JsonValue {
	if (reference.startsWith('params.')) {
		const key = reference.slice('params.'.length)
		if (!(key in state.params)) {
			throw new AppError('RESOLUTION_ERROR', `Missing parameter "${key}".`)
		}
		return state.params[key]
	}

	if (reference.startsWith('vars.')) {
		const key = reference.slice('vars.'.length)
		if (!(key in state.vars)) {
			throw new AppError('RESOLUTION_ERROR', `Missing variable "${key}".`)
		}
		return state.vars[key]
	}

	if (reference.startsWith('node.')) {
		const segments = reference.split('.')
		if (segments.length < 3) {
			throw new AppError('RESOLUTION_ERROR', `Invalid node reference "${reference}".`)
		}
		const nodeId = segments[1]
		const nodeOutput = state.nodeOutputs.get(nodeId)
		if (!nodeOutput) {
			throw new AppError(
				'RESOLUTION_ERROR',
				`Node reference "${reference}" does not resolve to a committed output.`,
			)
		}
		if (segments[2] === 'text') {
			if (nodeOutput.mode !== 'text') {
				throw new AppError(
					'RESOLUTION_ERROR',
					`Node reference "${reference}" does not resolve to text output.`,
				)
			}
			return nodeOutput.text
		}
		if (segments[2] === 'json') {
			if (nodeOutput.mode !== 'json') {
				throw new AppError(
					'RESOLUTION_ERROR',
					`Node reference "${reference}" does not resolve to json output.`,
				)
			}
			const path = segments.slice(3).join('.')
			if (!path) {
				return nodeOutput.json
			}
			return resolveJsonPath(nodeOutput.json, path, reference)
		}
		throw new AppError('RESOLUTION_ERROR', `Invalid node reference "${reference}".`)
	}

	if (reference.startsWith('event.')) {
		if (!state.event) {
			throw new AppError(
				'RESOLUTION_ERROR',
				`Event reference "${reference}" is unavailable for this run.`,
			)
		}
		const path = reference.slice('event.'.length)
		return resolveJsonPath(state.event, path, reference)
	}

	throw new AppError('RESOLUTION_ERROR', `Unsupported reference "${reference}".`)
}

export function resolveNodeInput(parts: InputPart[], state: RunStateSnapshot): string {
	const resolvedParts: string[] = []
	for (const part of parts) {
		if (part.type === 'text') {
			resolvedParts.push(part.text)
			continue
		}
		resolvedParts.push(stringifyInputValue(resolveInputReference(part.ref, state)))
	}
	return resolvedParts.join('')
}

export function buildNodeLookup(agentFile: AgentFile): Map<NodeId, AgentNode> {
	return new Map(agentFile.nodes.map((node) => [node.id, node] as const))
}
