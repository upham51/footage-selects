// A stand-in for Cloudflare D1 backed by Node's built-in SQLite, with the real
// migration applied, so the tests exercise the same SQL and constraints.
import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";

export function fakeD1({ failing = false } = {}) {
  const db = new DatabaseSync(":memory:");
  db.exec(readFileSync(new URL("../migrations/0001_waitlist.sql", import.meta.url), "utf8"));
  const statement = (sql, params = []) => ({
    bind: (...p) => statement(sql, p),
    async run() {
      if (failing) throw new Error("D1 unavailable");
      const r = db.prepare(sql).run(...params);
      return { success: true, meta: { changes: Number(r.changes) } };
    },
    async first() {
      if (failing) throw new Error("D1 unavailable");
      return db.prepare(sql).get(...params) ?? null;
    },
    async all() {
      if (failing) throw new Error("D1 unavailable");
      return { results: db.prepare(sql).all(...params) };
    },
  });
  return { prepare: (sql) => statement(sql), raw: db };
}
