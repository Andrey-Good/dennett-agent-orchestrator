import path from 'node:path'
import type { SQLiteLocalStateStore } from '../core/state/index.js'

type SQLiteLocalStateStoreConstructor =
	typeof import('../core/state/index.js').SQLiteLocalStateStore

let sqliteLocalStateStoreConstructor: SQLiteLocalStateStoreConstructor | null = null

async function getSQLiteLocalStateStoreConstructor(): Promise<SQLiteLocalStateStoreConstructor> {
	if (sqliteLocalStateStoreConstructor === null) {
		const stateModule = await import('../core/state/index.js')
		sqliteLocalStateStoreConstructor = stateModule.SQLiteLocalStateStore
	}
	return sqliteLocalStateStoreConstructor
}

export async function createLocalStateStore(
	stateDbPath: string,
): Promise<SQLiteLocalStateStore> {
	const SQLiteLocalStateStore = await getSQLiteLocalStateStoreConstructor()
	return new SQLiteLocalStateStore({
		database_path: path.resolve(process.cwd(), stateDbPath),
	})
}
