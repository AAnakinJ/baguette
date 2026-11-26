// Creates (or finds) the Stripe catalogue: the Pro product, 12 prices (3 offers x 4 country
// zones, VAT included), the founder coupon and the webhook. Safe to run again.
//
// Run it yourself, so the secret key never leaves your terminal:
//   $env:STRIPE_SECRET_KEY = "sk_test_..." or a restricted "rk_test_..."   (test mode first)
//   node server/scripts/stripe-setup.mjs https://<licence-server-address>
//
// It prints the STRIPE_PRICES value for wrangler.toml and, the first time, the webhook secret
// to store with `npx wrangler secret put STRIPE_WEBHOOK_SECRET`.
import { PRICES, OFFERS } from "../src/pricing.ts";
import { formEncode, STRIPE_API_VERSION } from "../src/stripe.ts";

const key = process.env.STRIPE_SECRET_KEY;
const apiBase = process.argv[2];
const product = process.env.PRODUCT_NAME ?? "Baguette";
if (!key || !/^(sk|rk)_(test|live)_/.test(key)) {
  console.error("Set STRIPE_SECRET_KEY first (sk_ or restricted rk_ key, test or live).");
  process.exit(1);
}

async function stripe(method, path, params) {
  const body = params ? formEncode(params).join("&") : undefined;
  const res = await fetch(`https://api.stripe.com/v1/${path}${method === "GET" && body ? `?${body}` : ""}`, {
    method,
    headers: { Authorization: `Bearer ${key}`, "Stripe-Version": STRIPE_API_VERSION, ...(method === "POST" ? { "Content-Type": "application/x-www-form-urlencoded" } : {}) },
    body: method === "POST" ? body : undefined,
  });
  const json = await res.json();
  if (!res.ok) throw new Error(`${path}: ${json?.error?.message ?? res.status}`);
  return json;
}

// Downloadable software, personal use: an eligible Managed Payments tax code.
const TAX_CODE = "txcd_10202000";

const found = await stripe("GET", "products/search", { query: "metadata['role']:'pro'" });
const prod =
  found.data[0] ??
  (await stripe("POST", "products", { name: `${product} Pro`, description: "Pro features: automatic proof, night pilot, freelance mode.", tax_code: TAX_CODE, metadata: { role: "pro" } }));
console.log(`product: ${prod.id}`);

const table = {};
for (const offer of OFFERS) {
  table[offer] = {};
  for (const zone of [1, 2, 3, 4]) {
    const lookup = `pro_${offer}_z${zone}`;
    const existing = (await stripe("GET", "prices", { lookup_keys: [lookup], active: true })).data[0];
    const price =
      existing ??
      (await stripe("POST", "prices", {
        product: prod.id,
        currency: "eur",
        unit_amount: Math.round(PRICES[zone][offer] * 100),
        tax_behavior: "inclusive",
        lookup_key: lookup,
        ...(offer === "lifetime" ? {} : { recurring: { interval: offer === "monthly" ? "month" : "year" } }),
        metadata: { offer, zone: String(zone) },
      }));
    table[offer][String(zone)] = price.id;
    console.log(`${lookup}: ${price.id}${existing ? " (already there)" : ""}`);
  }
}

// Founder price: -30 % for as long as the subscription lasts (the dates are set in wrangler.toml).
try {
  await stripe("GET", "coupons/FOUNDER30");
  console.log("coupon FOUNDER30: already there");
} catch {
  await stripe("POST", "coupons", { id: "FOUNDER30", percent_off: 30, duration: "forever", name: "Founder -30%" });
  console.log("coupon FOUNDER30: created");
}

if (apiBase) {
  const url = `${apiBase.replace(/\/$/, "")}/v1/stripe/webhook`;
  const hooks = await stripe("GET", "webhook_endpoints", { limit: 100 });
  if (hooks.data.some((h) => h.url === url)) {
    console.log(`webhook: already there (${url})`);
  } else {
    const hook = await stripe("POST", "webhook_endpoints", {
      url,
      api_version: STRIPE_API_VERSION,
      enabled_events: ["checkout.session.completed", "checkout.session.async_payment_succeeded", "customer.subscription.created", "customer.subscription.updated", "customer.subscription.deleted", "charge.refunded"],
    });
    console.log(`webhook: created (${url})`);
    console.log("\nWebhook secret, to store with: npx wrangler secret put STRIPE_WEBHOOK_SECRET --config server/wrangler.toml");
    console.log(hook.secret);
  }
}

console.log("\nSTRIPE_PRICES (not secret, give it to Claude for wrangler.toml):");
console.log(JSON.stringify(table));
