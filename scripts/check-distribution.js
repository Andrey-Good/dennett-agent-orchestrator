import { execFile } from 'node:child_process'
import { readdir } from 'node:fs/promises'
import path from 'node:path'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)
const distPath = path.resolve('dist')
const cliPath = path.join(distPath, 'src', 'interfaces', 'cli.js')

async function collectFiles(directory) {
	const entries = await readdir(directory, { withFileTypes: true })
	const files = []

	for (const entry of entries) {
		const entryPath = path.join(directory, entry.name)
		if (entry.isDirectory()) {
			files.push(...(await collectFiles(entryPath)))
		} else if (entry.isFile()) {
			files.push(entryPath)
		}
	}

	return files
}

const files = await collectFiles(distPath)
const normalizedFiles = files.map((filePath) => path.relative(process.cwd(), filePath).replace(/\\/g, '/'))

if (!normalizedFiles.includes('dist/src/interfaces/cli.js')) {
	throw new Error('Missing generated CLI artifact: dist/src/interfaces/cli.js')
}

const vitestConfigArtifacts = normalizedFiles.filter((filePath) =>
	/^dist\/vitest\.config\.(js|d\.ts|d\.ts\.map)$/.test(filePath),
)

if (vitestConfigArtifacts.length > 0) {
	throw new Error(`Build emitted forbidden vitest config artifacts: ${vitestConfigArtifacts.join(', ')}`)
}

const { stdout } = await execFileAsync(process.execPath, [cliPath, '--help'], {
	cwd: process.cwd(),
	windowsHide: true,
})

if (!stdout.includes('dennett-agent-orchestrator') || !stdout.includes('Usage:')) {
	throw new Error('Generated CLI help output did not contain the expected command identity.')
}
