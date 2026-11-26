// Read-only health check of the Stripe account (test or live, from STRIPE_SECRET_KEY):
// account country and status, and whether a Managed Payments checkout can be created.
// The test session it creates expires by itself; nothing is charged.
import { readFileSync } from "node:fs";
import { formEncode, STRIPE_API_VERSION } from "../src/stripe.ts";

const key = process.env.STRIPE_SECRET_KEY;
if (!key) {
  console.error("Set STRIPE_SECRET_KEY first.");
  process.exit(1);
}

async function stripe(method, path, params) {
  const body = params ? formEncode(params).join("&") : undefined;
  const res = await fetch(`https://api.stripe.com/v1/${path}`, {
    method,
    headers: { Authorization: `Bearer ${key}`, "Stripe-Version": STRIPE_API_VERSION, ...(body ? { "Content-Type": "application/x-www-form-urlencoded" } : {}) },
    body,
  });
  return { ok: res.ok, json: await res.json() };
}

const acct = await stripe("GET", "account");
const a = acct.json;
console.log(`account: country=${a.country} type=${a.business_type ?? "?"} charges_enabled=${a.charges_enabled} payouts_enabled=${a.payouts_enabled} details_submitted=${a.details_submitted}`);
if (a.requirements?.currently_due?.length) console.log(`still due: ${a.requirements.currently_due.join(", ")}`);

const prices = JSON.parse(readFileSync(new URL("../stripe-prices.test.json", import.meta.url), "utf8"));
for (const [mode, price] of [["subscription", prices.monthly["1"]], ["payment", prices.lifetime["1"]]]) {
  const r = await stripe("POST", "checkout/sessions", {
    mode,
    line_items: [{ price, quantity: 1 }],
    managed_payments: { enabled: true },
    success_url: "http://localhost:8787/v1/success?session_id={CHECKOUT_SESSION_ID}&lang=fr",
    cancel_url: "http://localhost:8787/",
    metadata: { offer: mode === "payment" ? "lifetime" : "monthly", zone: "1" },
  });
  console.log(r.ok ? `managed payments (${mode}): OK, session ${r.json.id}` : `managed payments (${mode}): REFUSED: ${r.json.error?.message}`);
}
