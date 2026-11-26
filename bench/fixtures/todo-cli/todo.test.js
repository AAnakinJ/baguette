import { test } from "node:test";
import assert from "node:assert/strict";
import { add, remove, format } from "./todo.js";

test("add assigns increasing ids", () => {
  const t = add(add([], "a"), "b");
  assert.deepEqual(t.map((x) => x.id), [1, 2]);
});

test("remove deletes by id", () => {
  assert.equal(remove(add([], "a"), 1).length, 0);
});

test("format lists todos", () => {
  assert.equal(format(add([], "a")), "1. a");
});
