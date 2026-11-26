// The Worker's HTTP layer with a fake Stripe: checkout redirect, webhook, thank-you page,
// activation from the app, CORS.
import { createHmac } from "node:crypto";
import { describe, expect, it } from "vitest";
import { createHandler, type Env } from "../src/index.ts";
import { zoneOf } from "../src/pricing.ts";
import { MemoryStore } from "../src/store.ts";
import { formEncode, verifyWebhook, type StripeApi } from "../src/stripe.ts";
import { DEVICE, ENC_SECRET, testSigner } from "./helpers.ts";

const NOW = 1_800_000_000;
const env: Env = {
  DB: null as never,
  SIGNING_KEY: "",
  KEY_ENC_SECRET: ENC_SECRET,
  STRIPE_SECRET_KEY: "sk_test_x",
  STRIPE_WEBHOOK_SECRET: "whsec_test",
  STRIPE_PRICES: JSON.stringify({ monthly: { "1": "price_m1", "3": "price_m3" }, lifetime: { "1": "price_l1" } }),
  SITE_URL: "https://site.test",
  API_BASE: "https://api.test",
  PRODUCT_NAME: "Baguette",
};

async function app(extra: Partial<Env> = {}) {
  const store = new MemoryStore();
  const calls: { method: string; path: string; params?: Record<string, unknown> }[] = [];
  const sessions: Record<string, Record<string, unknown>> = {
    cs_test_paid0000001: { id: "cs_test_paid0000001", mode: "subscription", payment_status: "paid", subscription: "sub_9", customer: "cus_9", metadata: { offer: "monthly" } },
  };
  const stripe: StripeApi = async (method, path, params) => {
    calls.push({ method, path, params });
    if (path === "checkout/sessions") return { id: "cs_test_new", url: "https://checkout.stripe.test/c/pay/cs_test_new" };
    if (path.startsWith("checkout/sessions/")) {
      const s = sessions[decodeURIComponent(path.split("/")[2])];
      if (!s) throw new Error("stripe 404");
      return s;
    }
    if (path.startsWith("subscriptions/")) return { id: "sub_9", status: "active", items: { data: [{ current_period_end: NOW + 30 * 86_400 }] } };
    throw new Error(`unexpected ${path}`);
  };
  const handler = createHandler({ env: { ...env, ...extra }, store, sign: await testSigner(), stripe, now: () => NOW });
  return { handler, store, calls };
}

const req = (path: string, init?: RequestInit & { country?: string }) =>
  new Request(`https://api.test${path}`, { ...init, headers: { ...(init?.country ? { "cf-ipcountry": init.country } : {}), ...(init?.headers ?? {}) } });

function signed(payload: string, secret = "whsec_test", t = NOW) {
  return `t=${t},v1=${createHmac("sha256", secret).update(`${t}.${payload}`).digest("hex")}`;
}

describe("stripe helpers", () => {
  it("encodes nested parameters", () => {
    expect(formEncode({ mode: "payment", line_items: [{ price: "p", quantity: 1 }], managed_payments: { enabled: true } }).join("&")).toBe(
      "mode=payment&line_items%5B0%5D%5Bprice%5D=p&line_items%5B0%5D%5Bquantity%5D=1&managed_payments%5Benabled%5D=true",
    );
  });
  it("checks webhook signatures and their age", async () => {
    const p = '{"id":"evt_1"}';
    expect(await verifyWebhook(p, signed(p), "whsec_test", NOW)).toBe(true);
    expect(await verifyWebhook(p, signed(p, "other"), "whsec_test", NOW)).toBe(false);
    expect(await verifyWebhook(`${p} `, signed(p), "whsec_test", NOW)).toBe(false);
    expect(await verifyWebhook(p, signed(p, "whsec_test", NOW - 600), "whsec_test", NOW)).toBe(false);
    expect(await verifyWebhook(p, null, "whsec_test", NOW)).toBe(false);
  });
});

describe("pricing zones", () => {
  it("adapt to the buyer's country", () => {
    expect([zoneOf("FR"), zoneOf("es"), zoneOf("BR"), zoneOf("IN"), zoneOf(null), zoneOf("ZZ")]).toEqual([1, 2, 3, 4, 1, 1]);
  });
});

describe("worker", () => {
  it("sends the buyer to Stripe Checkout with Managed Payments and the zone price", async () => {
    const { handler, calls } = await app();
    const res = await handler(req("/v1/checkout?offer=monthly&lang=fr", { country: "BR" }));
    expect(res.status).toBe(303);
    expect(res.headers.get("location")).toBe("https://checkout.stripe.test/c/pay/cs_test_new");
    const params = calls[0].params!;
    expect(params).toMatchObject({ mode: "subscription", line_items: [{ price: "price_m3", quantity: 1 }], managed_payments: { enabled: true }, locale: "fr", allow_promotion_codes: true });
    expect(String(params.success_url)).toContain("{CHECKOUT_SESSION_ID}");
    expect((await handler(req("/v1/checkout?offer=gratis"))).status).toBe(400);
  });

  it("applies the founder discount only during the founder period", async () => {
    const { handler, calls } = await app({ FOUNDER_COUPON: "FOUNDER30", FOUNDER_UNTIL: String(NOW + 10) });
    await handler(req("/v1/checkout?offer=monthly&lang=en"));
    expect(calls[0].params).toMatchObject({ discounts: [{ coupon: "FOUNDER30" }] });
    expect(calls[0].params).not.toHaveProperty("allow_promotion_codes");
  });

  it("webhook + thank-you page give the same key, which activates in the app", async () => {
    const { handler, store, calls } = await app();
    const event = JSON.stringify({ id: "evt_1", type: "checkout.session.completed", data: { object: { id: "cs_test_paid0000001", payment_status: "paid" } } });
    expect((await handler(req("/v1/stripe/webhook", { method: "POST", body: event, headers: { "stripe-signature": signed(event) } }))).status).toBe(200);
    expect((await handler(req("/v1/stripe/webhook", { method: "POST", body: event, headers: { "stripe-signature": "t=1,v1=00" } }))).status).toBe(400);
    expect(store.licences).toHaveLength(1);
    expect(store.licences[0].period_end).toBe(NOW + 30 * 86_400);

    const page = await handler(req("/v1/success?session_id=cs_test_paid0000001&lang=fr"));
    expect(page.status).toBe(200);
    const html = await page.text();
    const key = /PRO(-[2-9A-HJ-NP-Z]{4}){4}/.exec(html)![0];
    expect(html).toContain("Ta clé Pro");
    expect(store.licences).toHaveLength(1);

    const act = await handler(req("/v1/activate", { method: "POST", body: JSON.stringify({ key, device: DEVICE }) }));
    expect(act.status).toBe(200);
    expect(act.headers.get("access-control-allow-origin")).toBe("*");
    expect(await act.json()).toMatchObject({ kind: "pro" });

    expect(html).toContain("En cas de perte"); // no email promised while emails are not set up

    // Full refund: licence revoked AND subscription cancelled at Stripe (no renewal next year).
    const refund = JSON.stringify({ id: "evt_2", type: "charge.refunded", data: { object: { id: "ch_1", refunded: true, customer: "cus_9", payment_intent: "pi_x" } } });
    expect((await handler(req("/v1/stripe/webhook", { method: "POST", body: refund, headers: { "stripe-signature": signed(refund) } }))).status).toBe(200);
    expect(calls.some((c) => c.method === "DELETE" && c.path === "subscriptions/sub_9")).toBe(true);
    const again = await handler(req("/v1/activate", { method: "POST", body: JSON.stringify({ key, device: DEVICE }) }));
    expect([again.status, await again.json()]).toEqual([403, { error: "revoked" }]);

    const unknown = await handler(req("/v1/success?session_id=cs_test_unknown00001&lang=en"));
    expect(unknown.status).toBe(404);
    expect(await handler(req("/v1/success?session_id=<script>")).then((r) => r.status)).toBe(404);
  });

  it("answers the app: trial, flags, prices, preflight, errors as codes", async () => {
    const { handler } = await app();
    const trial = await handler(req("/v1/trial", { method: "POST", body: JSON.stringify({ device: DEVICE }) }));
    expect(await trial.json()).toMatchObject({ kind: "trial" });
    expect(await (await handler(req("/v1/flags"))).json()).toHaveProperty("token");
    expect(await (await handler(req("/v1/prices", { country: "IN" }))).json()).toMatchObject({ zone: 4, prices: { monthly: 3 }, lifetime_left: 500 });
    expect((await handler(req("/v1/activate", { method: "OPTIONS" }))).status).toBe(204);
    const bad = await handler(req("/v1/activate", { method: "POST", body: "{nope" }));
    expect([bad.status, await bad.json()]).toEqual([400, { error: "bad_json" }]);
    expect((await handler(req("/nothing"))).status).toBe(404);
  });
});
