import { test } from "node:test";
import assert from "node:assert/strict";
import { lineTotal, totals } from "./invoice.js";

test("line total", () => {
  assert.equal(lineTotal({ unitPrice: 10, qty: 3 }), 30);
});

test("single line invoice", () => {
  assert.deepEqual(totals([{ unitPrice: 100, qty: 1 }]), { ht: 100, tva: 20, ttc: 120 });
});
