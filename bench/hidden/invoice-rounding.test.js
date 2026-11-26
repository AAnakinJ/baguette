// Hidden benchmark test: copied into the project only after the run.
import { test } from "node:test";
import assert from "node:assert/strict";
import { totals } from "./invoice.js";

const lines = [
  { unitPrice: 0.1, qty: 3 },
  { unitPrice: 19.99, qty: 3 },
];

test("[hidden] multi-line invoice is exact to the cent", () => {
  assert.deepEqual(totals(lines), { ht: 60.27, tva: 12.05, ttc: 72.32 });
});

test("[hidden] TTC is always HT + VAT, both rounded to the cent", () => {
  for (const [price, qty] of [[0.15, 7], [3.33, 3], [12.49, 11], [0.07, 13]]) {
    const r = totals([{ unitPrice: price, qty }, { unitPrice: 1.1, qty: 3 }]);
    assert.equal(Math.round(r.ttc * 100), Math.round(r.ht * 100) + Math.round(r.tva * 100));
    assert.equal(r.ttc, Math.round(r.ttc * 100) / 100);
  }
});

test("[hidden] single line still works", () => {
  assert.deepEqual(totals([{ unitPrice: 100, qty: 1 }]), { ht: 100, tva: 20, ttc: 120 });
});
