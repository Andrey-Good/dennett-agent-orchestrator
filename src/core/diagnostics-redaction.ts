import { createHash } from 'node:crypto'
import type { JsonObject, JsonValue } from './json.js'

const REDACTED_EMAIL = '[REDACTED_EMAIL]'
const REDACTED_SECRET = '[REDACTED_SECRET]'
const REDACTED_PATH = '[REDACTED_PATH]'
const REDACTED_URL_CREDENTIALS = '[REDACTED_URL_CREDENTIALS]'

const SENSITIVE_KEY_PATTERN =
	/(?:api[_-]?key|access[_-]?token|refresh[_-]?token|id[_-]?token|token|secret|password|authorization|cookie|credential|private[_-]?key|client[_-]?secret)/i

const OMITTED_PAYLOAD_KEY_PATTERN =
	/(?:^|_)(prompt|reply|memory|transcript)(?:_|$)|runtime[_-]?handle|native[_-]?session[_-]?handle|request[_-]?handle/i

const PROVIDER_CONFIG_HINT_KEYS = new Set([
	'api_key',
	'apikey',
	'access_token',
	'refresh_token',
	'token',
	'secret',
	'password',
	'python_executable',
	'working_directory',
	'history_db_path',
	'mem0_config',
	'vector_store',
	'embedder',
	'llm',
	'endpoint',
	'base_url',
])

export function hashDiagnosticIdentifier(value: string): string {
	return createHash('sha256').update(value).digest('hex')
}

function redactedObject(reason: string): JsonObject {
	return {
		redacted: true,
		reason,
	}
}

function asJsonObject(value: JsonValue): JsonObject | null {
	if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
		return value
	}
	return null
}

function normalizeKey(key: string): string {
	return key.toLowerCase().replaceAll('-', '_')
}

function looksLikeProviderConfigObject(value: JsonValue): boolean {
	const object = asJsonObject(value)
	if (!object) {
		return false
	}
	return Object.keys(object).some((key) => PROVIDER_CONFIG_HINT_KEYS.has(normalizeKey(key)))
}

export function redactDiagnosticString(value: string): string {
	let redacted = value

	redacted = redacted.replace(
		/\b(https?:\/\/)([^/\s:@]+):([^/\s@]+)@/gi,
		`$1${REDACTED_URL_CREDENTIALS}@`,
	)
	redacted = redacted.replace(
		/(?<!\[)\b([a-z0-9._%+-]+)@([a-z0-9.-]+\.[a-z]{2,})\b/gi,
		REDACTED_EMAIL,
	)
	redacted = redacted.replace(
		/\b(?:Bearer|Basic)\s+[A-Za-z0-9._~+/=-]+/gi,
		`Authorization: ${REDACTED_SECRET}`,
	)
	redacted = redacted.replace(
		/\b(sk-[A-Za-z0-9_-]{8,}|ghp_[A-Za-z0-9_]{8,}|github_pat_[A-Za-z0-9_]+|xox[baprs]-[A-Za-z0-9-]{8,})\b/g,
		REDACTED_SECRET,
	)
	redacted = redacted.replace(
		/\b(api[_-]?key|access[_-]?token|refresh[_-]?token|token|password|secret)=([^&\s]+)/gi,
		`$1=${REDACTED_SECRET}`,
	)
	redacted = redacted.replace(
		/(?<![A-Za-z])[A-Za-z]:[\\/](?:[^\s"'<>|{}[\]]+[\\/]?)+/g,
		REDACTED_PATH,
	)
	redacted = redacted.replace(
		/(^|[\s"'(])\/(?:Users|home|private|tmp|var|mnt|opt)\/[^\s"'<>|{}[\]]+/g,
		(_match, prefix: string) => `${prefix}${REDACTED_PATH}`,
	)

	return redacted
}

export function redactDiagnosticsValue(value: JsonValue, keyHint?: string): JsonValue {
	if (keyHint && SENSITIVE_KEY_PATTERN.test(keyHint)) {
		return REDACTED_SECRET
	}
	if (keyHint && OMITTED_PAYLOAD_KEY_PATTERN.test(keyHint)) {
		return redactedObject(
			'Prompt, reply, memory, transcript, and runtime handle payloads are omitted.',
		)
	}
	if (keyHint && /config/i.test(keyHint) && looksLikeProviderConfigObject(value)) {
		return redactedObject('Provider configuration is local/private and omitted from diagnostics.')
	}

	if (typeof value === 'string') {
		return redactDiagnosticString(value)
	}
	if (Array.isArray(value)) {
		return value.map((entry) => redactDiagnosticsValue(entry))
	}
	const object = asJsonObject(value)
	if (object) {
		const redacted: JsonObject = {}
		for (const [key, entry] of Object.entries(object)) {
			redacted[key] = redactDiagnosticsValue(entry, key)
		}
		return redacted
	}
	return value
}
