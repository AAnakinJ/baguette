// Hidden benchmark test: copied into the project only after the run.
import { test } from "node:test";
import assert from "node:assert/strict";
import * as m from "./todo.js";

test("[hidden] done marks the right todo as completed", () => {
  assert.equal(typeof m.done, "function", "todo.js must export done(todos, id)");
  const t = m.done(m.add(m.add([], "a"), "b"), 2);
  assert.ok(t.find((x) => x.id === 2).done === true);
  assert.ok(!t.find((x) => x.id === 1).done);
});

test("[hidden] format shows completed todos with [x]", () => {
  assert.match(m.format(m.done(m.add([], "pain"), 1)), /\[x\]/);
});

test("[hidden] unknown id changes nothing", () => {
  const t = m.done(m.add([], "a"), 99);
  assert.equal(t.length, 1);
  assert.ok(!t[0].done);
});
