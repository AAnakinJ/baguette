// What can this Stripe key do? Read-only checks, plus writes that change nothing:
// a test checkout session (expires by itself) and re-saving the product's own metadata.
//   $env:STRIPE_SECRET_KEY = "rk_test_..." ; node server/scripts/stripe-permissions.mjs
import { readFileSync } from "node:fs";
import { formEncode, STRIPE_API_VERSION } from "../src/stripe.ts";

const key = process.env.STRIPE_SECRET_KEY ?? "";
if (!/^(sk|rk)_(test|live)_/.test(key)) {
  console.error("Set STRIPE_SECRET_KEY (sk_… or rk_…).");
  process.exit(1);
}
const live = key.includes("_live_");

async function call(method, path, params) {
  const body = params ? formEncode(params).join("&") : undefined;
  const res = await fetch(`https://api.stripe.com/v1/${path}${method === "GET" && body ? `?${body}` : ""}`, {
    method,
    headers: { Authorization: `Bearer ${key}`, "Stripe-Version": STRIPE_API_VERSION, ...(method === "POST" ? { "Content-Type": "application/x-www-form-urlencoded" } : {}) },
    body: method === "POST" ? body : undefined,
  });
  const json = await res.json();
  return { ok: res.ok, status: res.status, json };
}

const results = [];
const check = async (label, need, method, path, params) => {
  const r = await call(method, path, params);
  results.push({ label, need, ok: r.ok, why: r.ok ? "" : (r.json?.error?.message ?? String(r.status)).slice(0, 110) });
  return r;
};

await check("Account (read)", "checks", "GET", "account");
await check("Products (read)", "setup", "GET", "products", { limit: 1 });
await check("Prices (read)", "setup", "GET", "prices", { limit: 1 });
await check("Coupons (read)", "setup", "GET", "coupons", { limit: 1 });
await check("Webhook endpoints (read)", "setup", "GET", "webhook_endpoints", { limit: 1 });
await check("Checkout sessions (read)", "server", "GET", "checkout/sessions", { limit: 1 });
await check("Subscriptions (read)", "server", "GET", "subscriptions", { limit: 1, status: "all" });
await check("Customers (read)", "support", "GET", "customers", { limit: 1 });
await check("Charges (read)", "support", "GET", "charges", { limit: 1 });
await check("Refunds (read)", "support", "GET", "refunds", { limit: 1 });

if (!live) {
  const prices = JSON.parse(readFileSync(new URL("../stripe-prices.test.json", import.meta.url), "utf8"));
  await check("Checkout sessions (write)", "server", "POST", "checkout/sessions", {
    mode: "subscription",
    line_items: [{ price: prices.monthly["1"], quantity: 1 }],
    managed_payments: { enabled: true },
    success_url: "http://localhost:8787/v1/success?session_id={CHECKOUT_SESSION_ID}",
  });
  const prod = (await call("GET", "products/search", { query: "metadata['role']:'pro'" })).json?.data?.[0];
  if (prod) await check("Products (write)", "setup", "POST", `products/${prod.id}`, { metadata: { role: "pro" } });
}

console.log(`key: ${key.slice(0, 8)}… (${live ? "LIVE" : "test"})`);
for (const r of results) console.log(`${r.ok ? "OK " : "NO "} ${r.label.padEnd(28)} [${r.need}]${r.ok ? "" : `  ${r.why}`}`);
console.log("\nNot tested (they would change real objects): subscriptions write (cancel after a refund), refunds write, prices/coupons/webhooks write.");
