// Hidden benchmark test: copied into the project only after the run.
// Parses the output as RFC 4180 CSV instead of comparing strings, so any valid quoting passes.
import { test } from "node:test";
import assert from "node:assert/strict";
import * as m from "./todo.js";

function parseCsv(s) {
  const rows = [];
  let row = [];
  let field = "";
  let quoted = false;
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (quoted) {
      if (c === '"') {
        if (s[i + 1] === '"') {
          field += '"';
          i++;
        } else quoted = false;
      } else field += c;
    } else if (c === '"') quoted = true;
    else if (c === ",") {
      row.push(field);
      field = "";
    } else if (c === "\n" || c === "\r") {
      if (c === "\r" && s[i + 1] === "\n") i++;
      row.push(field);
      rows.push(row);
      row = [];
      field = "";
    } else field += c;
  }
  if (field !== "" || row.length) {
    row.push(field);
    rows.push(row);
  }
  return rows;
}

const TRUE = ["1", "true", "yes", "oui", "x"];
const FALSE = ["0", "false", "no", "non", ""];

test("[hidden] toCsv is exported and starts with the header", () => {
  assert.equal(typeof m.toCsv, "function", "todo.js must export toCsv(todos)");
  const rows = parseCsv(m.toCsv([]));
  assert.deepEqual(rows[0].map((s) => s.trim().toLowerCase()), ["id", "text", "done"]);
});

test("[hidden] commas and quotes keep the CSV valid", () => {
  const rows = parseCsv(m.toCsv([{ id: 1, text: 'pain, "complet"', done: true }, { id: 2, text: "croissant" }]));
  assert.equal(rows.length, 3);
  assert.equal(rows[1][1], 'pain, "complet"');
  assert.equal(rows[2][1], "croissant");
  assert.ok(TRUE.includes(rows[1][2].trim().toLowerCase()), `done=true written as ${rows[1][2]}`);
  assert.ok(FALSE.includes(rows[2][2].trim().toLowerCase()), `missing done written as ${rows[2][2]}`);
});

test("[hidden] a newline inside a task stays in one field", () => {
  const rows = parseCsv(m.toCsv([{ id: 7, text: "ligne1\nligne2" }]));
  assert.equal(rows.length, 2);
  assert.equal(rows[1][1], "ligne1\nligne2");
});
