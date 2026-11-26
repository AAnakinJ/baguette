// Hidden benchmark test: copied into the project only after the run.
import { test } from "node:test";
import assert from "node:assert/strict";
import * as m from "./todo.js";

test("[hidden] rename changes the text of the right todo", () => {
  assert.equal(typeof m.rename, "function", "todo.js must export rename(todos, id, text)");
  const t = m.rename(m.add(m.add([], "a"), "b"), 2, "c");
  assert.equal(t.find((x) => x.id === 2).text, "c");
  assert.equal(t.find((x) => x.id === 1).text, "a");
});

test("[hidden] rename with an unknown id changes nothing", () => {
  const t = m.rename(m.add([], "a"), 99, "z");
  assert.equal(t.length, 1);
  assert.equal(t[0].text, "a");
});

test("[hidden] high priority is shown with a leading !", () => {
  assert.equal(typeof m.setPriority, "function", "todo.js must export setPriority(todos, id, level)");
  assert.match(m.format(m.setPriority(m.add([], "bread"), 1, "high")), /^!/m);
});

test("[hidden] low priority and plain todos keep the usual format", () => {
  assert.doesNotMatch(m.format(m.setPriority(m.add([], "bread"), 1, "low")), /^!/m);
  assert.equal(m.format(m.add([], "a")), "1. a");
});
