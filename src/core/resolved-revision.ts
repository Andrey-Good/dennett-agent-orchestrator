import { createHash } from 'node:crypto'
import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

export async function computeResolvedRevisionId(agentFilePath: string): Promise<string> {
	const resolvedPath = path.resolve(agentFilePath)
	const rawAgentFile = await readFile(resolvedPath)
	const digest = createHash('sha256').update(rawAgentFile).digest('hex')
	return `${pathToFileURL(resolvedPath).href}#sha256:${digest}`
}
