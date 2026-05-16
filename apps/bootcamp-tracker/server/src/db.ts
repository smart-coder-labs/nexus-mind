import Database from 'better-sqlite3';
import path from 'path';
import { SCHEMA_SQL } from './schema';

const DB_PATH = path.join(__dirname, '..', 'data', 'bootcamp.db');

let _db: Database.Database | null = null;

export function getDb(): Database.Database {
  if (!_db) {
    _db = new Database(DB_PATH);
    _db.pragma('journal_mode = WAL');
    _db.pragma('foreign_keys = ON');
    _db.exec(SCHEMA_SQL);
  }
  return _db;
}
