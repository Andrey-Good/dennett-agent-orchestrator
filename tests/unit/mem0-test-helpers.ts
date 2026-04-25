import { mkdir, readFile, rm, stat, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'

const MEM0_CHROMA_LOCK_DIR = path.join(os.tmpdir(), 'dennett-mem0-chroma-vitest.lock')
const LOCK_POLL_MS = 250
const LOCK_TIMEOUT_MS = 10 * 60 * 1000
const STALE_LOCK_MS = 10 * 60 * 1000
let processLockDepth = 0

interface Mem0ChromaTestLock {
	release: () => Promise<void>
}

interface Mem0ChromaLockOwner {
	pid?: number
}

function isFilesystemBusyError(error: unknown): boolean {
	return (
		error !== null &&
		typeof error === 'object' &&
		'code' in error &&
		['EBUSY', 'ENOTEMPTY', 'EPERM'].includes(String((error as { code?: string }).code))
	)
}

function isExistingLockError(error: unknown): boolean {
	return (
		error !== null &&
		typeof error === 'object' &&
		'code' in error &&
		(error as { code?: string }).code === 'EEXIST'
	)
}

function isProcessRunning(pid: number): boolean {
	try {
		process.kill(pid, 0)
		return true
	} catch {
		return false
	}
}

async function readLockOwner(): Promise<Mem0ChromaLockOwner | undefined> {
	try {
		const ownerText = await readFile(path.join(MEM0_CHROMA_LOCK_DIR, 'owner.json'), 'utf8')
		const owner = JSON.parse(ownerText) as unknown
		if (owner !== null && typeof owner === 'object') {
			return owner as Mem0ChromaLockOwner
		}
	} catch {
		return undefined
	}
	return undefined
}

export async function acquireMem0ChromaTestLock(label: string): Promise<Mem0ChromaTestLock> {
	const startedAt = Date.now()

	if (processLockDepth > 0) {
		processLockDepth += 1
		return {
			async release() {
				processLockDepth = Math.max(0, processLockDepth - 1)
				if (processLockDepth === 0) {
					await rm(MEM0_CHROMA_LOCK_DIR, { recursive: true, force: true })
				}
			},
		}
	}

	while (true) {
		try {
			await mkdir(MEM0_CHROMA_LOCK_DIR)
			await writeFile(
				path.join(MEM0_CHROMA_LOCK_DIR, 'owner.json'),
				JSON.stringify({
					label,
					pid: process.pid,
					acquired_at: new Date().toISOString(),
				}),
			)
			processLockDepth = 1
			let released = false
			return {
				async release() {
					if (released) {
						return
					}
					released = true
					processLockDepth = Math.max(0, processLockDepth - 1)
					if (processLockDepth === 0) {
						await rm(MEM0_CHROMA_LOCK_DIR, { recursive: true, force: true })
					}
				},
			}
		} catch (error) {
			if (!isExistingLockError(error)) {
				throw error
			}

			if (Date.now() - startedAt > LOCK_TIMEOUT_MS) {
				throw new Error(`Timed out waiting for Mem0 Chroma test lock for ${label}.`)
			}

			const lockOwner = await readLockOwner()
			if (
				typeof lockOwner?.pid === 'number' &&
				Number.isSafeInteger(lockOwner.pid) &&
				lockOwner.pid > 0 &&
				!isProcessRunning(lockOwner.pid)
			) {
				await rm(MEM0_CHROMA_LOCK_DIR, { recursive: true, force: true })
				continue
			}

			try {
				const lockStats = await stat(MEM0_CHROMA_LOCK_DIR)
				if (!lockOwner && Date.now() - lockStats.mtimeMs > STALE_LOCK_MS) {
					await rm(MEM0_CHROMA_LOCK_DIR, { recursive: true, force: true })
					continue
				}
			} catch {
				continue
			}

			await delay(LOCK_POLL_MS)
		}
	}
}

export async function cleanupMem0TempDir(tempDir: string): Promise<void> {
	for (let attempt = 0; attempt < 20; attempt += 1) {
		try {
			await rm(tempDir, { recursive: true, force: true })
			return
		} catch (error) {
			if (!isFilesystemBusyError(error) || attempt === 19) {
				throw error
			}
			await delay(Math.min(1000, 100 * (attempt + 1)))
		}
	}
}
