// Static website generator, no dependencies: `node site/build.mjs [--production]`.
// Output: site/dist, with relative links only, so it works on GitHub Pages (project sites under
// /<repo>/), Cloudflare Pages or any static host.
// Early access (config.earlyAccess): no prices, no checkout, no terms of sale, and no personal
// identity needed. Paid mode: --production refuses to build without legal.local.json.
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync, copyFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import content from "./content.mjs";
import { EARLY_PAGES, LEGAL_PAGES, earlyPage, legalPage } from "./legal.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const RTL = new Set(["ar"]);
const LOCALE = { pt: "pt-BR" };
/** FAQ entries about cancelling, refunds and "lifetime": hidden while nothing is sold. */
const PAID_ONLY_FAQ = new Set([4, 5, 6]);

const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

const LOGO = `<svg class="logo" width="22" height="22" viewBox="0 0 24 24" aria-hidden="true"><rect x="1" y="1" width="22" height="22" rx="5" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="M7.5 16.5 L16.5 7.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/><circle cx="7" cy="17" r="2.1" fill="currentColor"/></svg>`;

function money(lang, value) {
  return new Intl.NumberFormat(LOCALE[lang] ?? lang, { style: "currency", currency: "EUR", minimumFractionDigits: 0, maximumFractionDigits: 2 }).format(value);
}

/** Content-Security-Policy as a meta tag too: GitHub Pages cannot send headers. */
const csp = (cfg) => `default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'${cfg.earlyAccess ? "" : ` ${new URL(cfg.api).origin}`}; base-uri 'self'; form-action 'self'`;

// `up` is the way back to the site root from the page ("" for the root, "../" in a language folder).
function head(cfg, lang, title, description, path, up, langs = cfg.langs) {
  const alternates = langs.map((l) => `<link rel="alternate" hreflang="${l}" href="${cfg.site}/${l}/${path}">`).join("");
  return `<!doctype html><html lang="${lang}" dir="${RTL.has(lang) ? "rtl" : "ltr"}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="${csp(cfg)}">
<title>${esc(title)}</title><meta name="description" content="${esc(description)}">
<meta property="og:title" content="${esc(title)}"><meta property="og:description" content="${esc(description)}"><meta property="og:type" content="website">
<link rel="icon" href="${up}favicon.svg" type="image/svg+xml"><link rel="stylesheet" href="${up}style.css">${alternates}<link rel="alternate" hreflang="x-default" href="${cfg.site}/en/${path}"></head>`;
}

function langMenu(cfg, lang, path, up, legalLangs) {
  // Legal pages exist in French and English only: other languages lead to their home page.
  const href = (l) => (path && !legalLangs.includes(l) ? `${up}${l}/` : `${up}${l}/${path}`);
  const items = cfg.langs.map((l) => `<li><a href="${href(l)}" lang="${l}"${l === lang ? ' aria-current="true"' : ""}>${esc(content[l].langName)}</a></li>`).join("");
  return `<details class="langs"><summary>${esc(content[lang].langName)}</summary><ul>${items}</ul></details>`;
}

function topbar(cfg, c, lang, path, home, up) {
  const anchor = (id) => (home ? `#${id}` : `./#${id}`);
  const pricing = cfg.earlyAccess ? c.early.title : c.nav.pricing;
  return `<header class="top"><a class="brand" href="./">${LOGO}<span>${esc(cfg.product)}</span></a>
<nav><a href="${anchor("features")}">${esc(c.nav.features)}</a><a href="${anchor("pricing")}">${esc(pricing)}</a><a href="${anchor("faq")}">${esc(c.nav.faq)}</a>${langMenu(cfg, lang, path, up, ["fr", "en"])}<a class="btn small" href="${esc(cfg.download)}">${esc(c.nav.download)}</a></nav></header>`;
}

function footer(cfg, c, lang, up) {
  const legalLang = lang === "fr" ? "fr" : "en";
  const base = lang === legalLang ? "" : `${up}${legalLang}/`;
  const pages = cfg.earlyAccess ? [["privacy", c.footer.privacy], ["licence", c.footer.licence]] : [["terms", c.footer.terms], ["privacy", c.footer.privacy], ["licence", c.footer.licence]];
  const links = pages.map(([id, label]) => `<a href="${base}${id}.html">${esc(label)}</a>`).join("");
  return `<footer class="foot"><div class="links">${links}<a href="${esc(cfg.source)}">${esc(c.footer.source)}</a></div>
<p>${esc(c.footer.notAffiliated)}${lang !== "fr" && lang !== "en" ? ` ${esc(c.footer.legalLang)}` : ""}</p></footer>`;
}

function pricingSection(cfg, c, lang) {
  const list = (items) => `<ul class="ticks">${items.map((i) => `<li>${esc(i)}</li>`).join("")}</ul>`;
  if (cfg.earlyAccess) {
    return `<section id="pricing" class="early"><h2>${esc(c.early.title)}</h2><p class="lead">${esc(c.early.text)}</p>
<div class="ctas"><a class="btn big" href="${esc(cfg.download)}">${esc(c.hero.ctaDownload)}</a></div></section>`;
  }
  const buy = (offer) => `${cfg.api}/v1/checkout?offer=${offer}&amp;lang=${lang}`;
  const plan = (offer, name, price, unit, extra, featured) => `<div class="plan${featured ? " featured" : ""}">
${featured ? `<span class="ribbon">${esc(c.pricing.recommended)}</span>` : ""}<h3>${esc(name)}</h3>
<p class="price"><span data-price="${offer}">${money(lang, price)}</span><span class="unit">${esc(unit)}</span></p>${extra}
${list(c.pricing.proItems)}<a class="btn" href="${buy(offer)}" rel="nofollow">${esc(c.pricing.buy)}</a></div>`;
  return `<section id="pricing"><h2>${esc(c.pricing.title)}</h2><p class="lead">${esc(c.pricing.sub)}</p>
<div class="plans">
<div class="plan"><h3>${esc(c.pricing.free)}</h3><p class="price">${money(lang, 0)}</p><p class="muted small">${esc(c.pricing.freeDesc)}</p>${list(c.pricing.freeItems)}<a class="btn ghost" href="${esc(cfg.download)}">${esc(c.pricing.startFree)}</a></div>
${plan("monthly", c.pricing.monthly, 9, c.pricing.perMonth, "", false)}
${plan("annual", c.pricing.annual, 79, c.pricing.perYear, "", true)}
${plan("lifetime", c.pricing.lifetime, 149, ` ${c.pricing.once}`, `<p class="muted small" data-lifetime-left data-template="${esc(c.pricing.lifetimeLeft)}" data-sold-out="${esc(c.pricing.soldOut)}"></p>`, false)}
</div>
<ul class="promises"><li>${esc(c.pricing.trial)}</li><li>${esc(c.pricing.refund)}</li><li>${esc(c.pricing.zones)}</li></ul></section>`;
}

function home(cfg, lang) {
  const c = content[lang];
  const up = "../";
  const sub = (s) => esc(s).replaceAll("{product}", esc(cfg.product));
  const steps = c.how.steps.map(([t, d], i) => `<li><span class="n">${i + 1}</span><h3>${esc(t)}</h3><p>${esc(d)}</p></li>`).join("");
  const feats = c.feat.items.map((f) => `<li><h3>${esc(f.t)}${f.pro && !cfg.earlyAccess ? ` <span class="tag">${esc(c.feat.pro)}</span>` : ""}</h3><p>${esc(f.d)}</p></li>`).join("");
  const faqItems = cfg.earlyAccess ? [...c.faq.items.filter((_, i) => !PAID_ONLY_FAQ.has(i)), { q: c.early.q, a: c.early.a }] : c.faq.items;
  const faq = faqItems.map((f) => `<details><summary>${sub(f.q)}</summary><p>${sub(f.a)}</p></details>`).join("");
  const second = cfg.earlyAccess ? `<a class="btn ghost big" href="${esc(cfg.source)}">${esc(c.footer.source)}</a>` : `<a class="btn ghost big" href="#pricing">${esc(c.hero.ctaPro)}</a>`;
  const api = cfg.earlyAccess ? "" : ` data-api="${esc(cfg.api)}"`;
  return `${head(cfg, lang, c.meta.title.replaceAll("{product}", cfg.product), c.meta.description, "", up)}
<body${api} data-lang="${lang}">${topbar(cfg, c, lang, "", true, up)}
<main>
<section class="hero"><p class="kicker">${esc(c.hero.kicker)}</p><h1>${esc(c.hero.title)}</h1><p class="lead">${esc(c.hero.subtitle)}</p>
<div class="ctas"><a class="btn big" href="${esc(cfg.download)}">${esc(c.hero.ctaDownload)}</a>${second}</div>
<p class="muted small">${esc(c.hero.note)}</p></section>
<section id="how"><h2>${esc(c.how.title)}</h2><ol class="steps">${steps}</ol></section>
<section id="features"><h2>${esc(c.feat.title)}</h2><ul class="grid">${feats}</ul></section>
<section class="bench"><h2>${esc(c.bench.title)}</h2><p class="lead">${sub(c.bench.text)}</p><p class="muted small">${esc(c.bench.note)}</p></section>
${pricingSection(cfg, c, lang)}
<section id="faq"><h2>${esc(c.faq.title)}</h2><div class="faq">${faq}</div></section>
</main>${footer(cfg, c, lang, up)}<script src="${up}app.js" defer></script></body></html>`;
}

function legal(cfg, lang, id, vars) {
  const c = content[lang];
  const up = "../";
  const page = cfg.earlyAccess ? earlyPage(lang, id, vars) : legalPage(lang, id, vars);
  return `${head(cfg, lang, `${page.title} — ${cfg.product}`, page.title, `${id}.html`, up, ["fr", "en"])}<body>${topbar(cfg, c, lang, `${id}.html`, false, up)}
<main class="doc"><h1>${esc(page.title)}</h1>${page.body}</main>${footer(cfg, c, lang, up)}</body></html>`;
}

function root(cfg) {
  const links = cfg.langs.map((l) => `<li><a href="${l}/" lang="${l}">${esc(content[l].langName)}</a></li>`).join("");
  return `${head(cfg, "en", cfg.product, content.en.meta.description.replaceAll("{product}", cfg.product), "", "")}<body class="root"><main class="doc"><h1>${LOGO} ${esc(cfg.product)}</h1><ul class="rootlangs">${links}</ul></main><script src="lang.js"></script></body></html>`;
}

function identity(production, early) {
  const file = join(HERE, "legal.local.json");
  if (existsSync(file)) return JSON.parse(readFileSync(file, "utf8"));
  if (production && !early) throw new Error("legal.local.json is missing: the legal identity is required to publish (see legal.example.json)");
  const todo = (what) => `<mark>[À compléter : ${what}]</mark>`;
  return { name: todo("nom et prénom"), email: "contact@example.invalid", mediator: "" };
}

export function build({ outDir = join(HERE, "dist"), production = false, today = new Date(), earlyAccess } = {}) {
  const cfg = JSON.parse(readFileSync(join(HERE, "config.json"), "utf8"));
  if (earlyAccess !== undefined) cfg.earlyAccess = earlyAccess;
  const vars = { ...identity(production, cfg.earlyAccess), product: cfg.product, updated: today.toISOString().slice(0, 10), source: cfg.source, issues: `${cfg.source}/issues` };
  rmSync(outDir, { recursive: true, force: true });
  mkdirSync(outDir, { recursive: true });
  for (const f of ["style.css", "app.js", "lang.js", "favicon.svg"]) copyFileSync(join(HERE, "static", f), join(outDir, f));
  writeFileSync(join(outDir, "index.html"), root(cfg));
  for (const lang of cfg.langs) {
    mkdirSync(join(outDir, lang), { recursive: true });
    writeFileSync(join(outDir, lang, "index.html"), home(cfg, lang));
  }
  const pages = cfg.earlyAccess ? EARLY_PAGES : LEGAL_PAGES;
  for (const lang of ["fr", "en"]) for (const page of pages) writeFileSync(join(outDir, lang, `${page}.html`), legal(cfg, lang, page, vars));
  writeFileSync(
    join(outDir, "_headers"),
    `/*\n  Content-Security-Policy: ${csp(cfg)}; frame-ancestors 'none'\n  X-Content-Type-Options: nosniff\n  Referrer-Policy: strict-origin-when-cross-origin\n  Permissions-Policy: camera=(), microphone=(), geolocation=()\n`,
  );
  // GitHub Pages: serve files as they are (no Jekyll processing).
  writeFileSync(join(outDir, ".nojekyll"), "");
  return { outDir, pages: cfg.langs.length + 1 + 2 * pages.length };
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const r = build({ production: process.argv.includes("--production") });
  console.log(`site built: ${r.pages} pages in ${r.outDir}`);
}
