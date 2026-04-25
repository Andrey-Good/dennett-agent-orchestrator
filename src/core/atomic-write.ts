import { randomUUID } from 'node:crypto'
import { mkdir, open, rename, rm } from 'node:fs/promises'
import path from 'node:path'

async function syncDirectoryIfPossible(directoryPath: string): Promise<void> {
	try {
		const directoryHandle = await open(directoryPath, 'r')
		try {
			await directoryHandle.sync()
		} finally {
			await directoryHandle.close()
		}
	} catch {
		// Some platforms do not allow directory fsync; best-effort only.
	}
}

export async function writeTextFileAtomically(targetPath: string, contents: string): Promise<void> {
	const targetDirectory = path.dirname(targetPath)
	const tempPath = path.join(targetDirectory, `${path.basename(targetPath)}.${randomUUID()}.tmp`)

	await mkdir(targetDirectory, { recursive: true })

	try {
		const tempHandle = await open(tempPath, 'w')
		try {
			await tempHandle.writeFile(contents, 'utf8')
			await tempHandle.sync()
		} finally {
			await tempHandle.close()
		}

		await rename(tempPath, targetPath)
		await syncDirectoryIfPossible(targetDirectory)
	} catch (error) {
		await rm(tempPath, { force: true }).catch(() => {})
		throw error
	}
}
