import { describe, expect, it } from "vitest";
import en from "./locales/en.json";
import es from "./locales/es.json";
import pt from "./locales/pt.json";
import fr from "./locales/fr.json";
import de from "./locales/de.json";
import ja from "./locales/ja.json";
import ko from "./locales/ko.json";
import hi from "./locales/hi.json";
import ar from "./locales/ar.json";
import id from "./locales/id.json";

type Tree = { [k: string]: string | Tree };

function flatten(tree: Tree, prefix = ""): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(tree)) {
    const key = prefix ? `${prefix}.${k}` : k;
    if (typeof v === "string") out[key] = v;
    else Object.assign(out, flatten(v, key));
  }
  return out;
}

const placeholders = (s: string) => [...s.matchAll(/\{\{(\w+)\}\}/g)].map((m) => m[1]).sort();

const reference = flatten(en as Tree);
const locales: Record<string, Tree> = { es, pt, fr, de, ja, ko, hi, ar, id };

describe("translations", () => {
  it("has the 10 interface languages", () => {
    expect(Object.keys(locales).length + 1).toBe(10);
  });

  for (const [code, tree] of Object.entries(locales)) {
    const flat = flatten(tree);
    it(`${code}: same keys as English`, () => {
      expect(Object.keys(flat).sort()).toEqual(Object.keys(reference).sort());
    });
    it(`${code}: same {{placeholders}} as English`, () => {
      for (const [k, v] of Object.entries(reference)) {
        expect(placeholders(flat[k] ?? ""), `${code}:${k}`).toEqual(placeholders(v));
      }
    });
    it(`${code}: nothing left empty`, () => {
      for (const [k, v] of Object.entries(flat)) expect(v.trim().length, `${code}:${k}`).toBeGreaterThan(0);
    });
  }
});
