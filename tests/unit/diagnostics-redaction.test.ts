import { describe, expect, it } from 'vitest'
import {
	redactDiagnosticString,
	redactDiagnosticsValue,
} from '../../src/core/diagnostics-redaction.js'

describe('diagnostics redaction', () => {
	it('redacts emails, authorization headers, tokens, and API key parameters', () => {
		const redacted = redactDiagnosticString(
			'user alice@example.com used Authorization: Bearer sk-secret123456789 with api_key=ghp_secret123456789',
		)

		expect(redacted).not.toContain('alice@example.com')
		expect(redacted).not.toContain('sk-secret123456789')
		expect(redacted).not.toContain('ghp_secret123456789')
		expect(redacted).toContain('[REDACTED_EMAIL]')
		expect(redacted).toContain('[REDACTED_SECRET]')
	})

	it('redacts credentialed URLs', () => {
		const redacted = redactDiagnosticString('https://alice:secret@example.com/private/repo.git')

		expect(redacted).not.toContain('alice:secret')
		expect(redacted).toBe('https://[REDACTED_URL_CREDENTIALS]@example.com/private/repo.git')
	})

	it('redacts Windows and POSIX absolute paths', () => {
		const redacted = redactDiagnosticString(
			'paths C:\\Users\\Alice\\project\\state.sqlite and /home/alice/project/state.sqlite',
		)

		expect(redacted).not.toContain('C:\\Users\\Alice')
		expect(redacted).not.toContain('/home/alice')
		expect(redacted).toContain('[REDACTED_PATH]')
	})

	it('omits provider config objects instead of partially exposing them', () => {
		const redacted = redactDiagnosticsValue({
			provider: {
				config: {
					api_key: 'sk-provider-secret123',
					python_executable: 'C:\\Users\\Alice\\private\\python.exe',
					mem0_config: {
						vector_store: {
							provider: 'chroma',
						},
					},
				},
			},
		})
		const serialized = JSON.stringify(redacted)

		expect(serialized).not.toContain('sk-provider-secret123')
		expect(serialized).not.toContain('python.exe')
		expect(serialized).not.toContain('mem0_config')
		expect(redacted).toEqual({
			provider: {
				config: {
					redacted: true,
					reason: 'Provider configuration is local/private and omitted from diagnostics.',
				},
			},
		})
	})

	it('omits prompt, reply, memory, transcript, and runtime handle payloads', () => {
		const redacted = redactDiagnosticsValue({
			prompt_payload: { text: 'secret prompt' },
			reply_payload: { text: 'secret reply' },
			memory_contents: [{ content: 'secret memory' }],
			runtime_handle: { thread_id: 'thread-secret' },
		})
		const serialized = JSON.stringify(redacted)

		expect(serialized).not.toContain('secret prompt')
		expect(serialized).not.toContain('secret reply')
		expect(serialized).not.toContain('secret memory')
		expect(serialized).not.toContain('thread-secret')
		expect(serialized).toContain('redacted')
	})
})
