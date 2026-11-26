import { existsSync, mkdtempSync, readdirSync, readFileSync, renameSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { build } from "./build.mjs";
import content from "./content.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const LANGS = ["en", "fr", "es", "pt", "de", "ja", "ko", "hi", "ar", "id"];

function site(earlyAccess: boolean) {
  const out = mkdtempSync(join(tmpdir(), "site-"));
  build({ outDir: out, today: new Date("2026-11-26"), earlyAccess });
  return { out, page: (p: string) => readFileSync(join(out, p), "utf8") };
}

/** Every relative link of every page points to a file that exists. */
function checkLinks(out: string) {
  const files: string[] = [];
  const walk = (d: string) => readdirSync(d, { withFileTypes: true }).forEach((e) => (e.isDirectory() ? walk(join(d, e.name)) : files.push(join(d, e.name))));
  walk(out);
  for (const file of files.filter((f) => f.endsWith(".html"))) {
    for (const [, href] of readFileSync(file, "utf8").matchAll(/(?:href|src)="([^"#]+?)(#[^"]*)?"/g)) {
      if (/^(https?:|mailto:)/.test(href)) continue;
      expect(href.startsWith("/"), `${file}: ${href} must be relative (GitHub Pages)`).toBe(false);
      const target = resolve(dirname(file), href);
      const asFile = href.endsWith("/") || existsSync(join(target, "index.html")) ? join(target, "index.html") : target;
      expect(existsSync(asFile), `${file} -> ${href}`).toBe(true);
    }
  }
}

describe("website, early access (now)", () => {
  const { out, page } = site(true);

  it("has a home page in each of the 10 languages, with no price and no checkout", () => {
    for (const l of LANGS) {
      const html = page(`${l}/index.html`);
      expect(html).toContain(`<html lang="${l}" dir="${l === "ar" ? "rtl" : "ltr"}">`);
      expect(html).not.toMatch(/checkout|data-price|data-api|class="plan/);
      expect(html).not.toMatch(/\{product\}|undefined|\{\{|NaN/);
    }
    const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/'/g, "&#39;");
    for (const l of LANGS) {
      expect(page(`${l}/index.html`)).toContain(esc(content[l].early.title));
      expect(page(`${l}/index.html`)).toContain(esc(content[l].early.q));
    }
  });

  it("publishes no personal identity and no terms of sale", () => {
    for (const l of ["fr", "en"]) {
      expect(existsSync(join(out, l, "terms.html"))).toBe(false);
      for (const p of ["privacy", "licence"]) expect(page(`${l}/${p}.html`)).not.toMatch(/To fill in|example\.invalid/);
    }
    expect(page("en/privacy.html")).toContain("collects no personal data");
    expect(page("en/licence.html")).toContain("FSL-1.1-ALv2");
  });

  it("only uses relative links to pages that exist", () => checkLinks(out));

  it("sends a strict security policy and no tracking, even on GitHub Pages", () => {
    expect(page("fr/index.html")).toContain('http-equiv="Content-Security-Policy"');
    expect(existsSync(join(out, ".nojekyll"))).toBe(true);
    expect(page("fr/index.html")).not.toMatch(/google|analytics|gtag|facebook|cookie/i);
  });
});

describe("website, paid mode (later)", () => {
  const { out, page } = site(false);

  it("shows prices, checkout links and the terms of sale", () => {
    expect(page("fr/index.html")).toContain("/v1/checkout?offer=annual&amp;lang=fr");
    expect(page("en/terms.html")).toContain("full refund of any payment requested within 14 days");
    checkLinks(out);
  });

  it("refuses to publish without the legal identity", () => {
    const local = join(HERE, "legal.local.json");
    const moved = existsSync(local) ? `${local}.bak-test` : null;
    if (moved) renameSync(local, moved);
    try {
      expect(() => build({ outDir: mkdtempSync(join(tmpdir(), "site-")), production: true, earlyAccess: false })).toThrow(/legal.local.json/);
    } finally {
      if (moved) renameSync(moved, local);
    }
  });
});
