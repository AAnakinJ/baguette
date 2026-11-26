// A D1-compatible database on Node's built-in SQLite, to run the real SQL of D1Store in tests
// and in the local dev server (no Cloudflare tooling needed).
import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";

const SCHEMA = new URL("../schema.sql", import.meta.url);

export function nodeD1(path = ":memory:") {
  const db = new DatabaseSync(path);
  db.exec(readFileSync(SCHEMA, "utf8"));
  return {
    prepare(sql) {
      const stmt = db.prepare(sql);
      return {
        bind(...values) {
          const v = values.map((x) => (x === undefined ? null : x));
          return {
            async first() {
              return stmt.get(...v) ?? null;
            },
            async all() {
              return { results: stmt.all(...v) };
            },
            async run() {
              const r = stmt.run(...v);
              return { meta: { changes: Number(r.changes) } };
            },
          };
        },
      };
    },
  };
}
