// Licence server (Cloudflare Worker, free plan): trials, keys, Stripe checkout and webhooks,
// remote flags. Public JSON endpoints under /v1, called by the desktop app and the website.
import { resendMailer, successPage } from "./pages.ts";
import { LIFETIME_CAP, PRICES, isOffer, priceId, zoneOf, type Offer } from "./pricing.ts";
import { HttpError, Service } from "./service.ts";
import { D1Store, type D1Like, type Store } from "./store.ts";
import { checkoutLocale, stripeApi, verifyWebhook, type StripeApi } from "./stripe.ts";
import { signerFromJwk, type Signer } from "./token.ts";

export interface Env {
  DB: D1Like;
  /** Ed25519 private key, JWK JSON (secret). */
  SIGNING_KEY: string;
  /** 32 random bytes, base64url (secret): encrypts stored keys. */
  KEY_ENC_SECRET: string;
  STRIPE_SECRET_KEY: string;
  STRIPE_WEBHOOK_SECRET: string;
  /** {"monthly":{"1":"price_…"},…} */
  STRIPE_PRICES: string;
  STRIPE_API_VERSION?: string;
  SITE_URL: string;
  API_BASE: string;
  PRODUCT_NAME?: string;
  /** "off" turns the subscription mode off in every app (kill switch). */
  SUBSCRIPTION_MODE?: string;
  FLAGS_MESSAGE?: string;
  FOUNDER_COUPON?: string;
  /** Unix seconds: the founder discount applies to checkouts before this date. */
  FOUNDER_UNTIL?: string;
  RESEND_API_KEY?: string;
  MAIL_FROM?: string;
}

export interface AppDeps {
  env: Env;
  store: Store;
  sign: Signer;
  stripe: StripeApi;
  now: () => number;
}

const LANGS = new Set(["en", "fr", "es", "pt", "de", "ja", "ko", "hi", "ar", "id"]);
const lang = (v: string | null) => (v && LANGS.has(v) ? v : "en");

const CORS = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type",
  "Access-Control-Max-Age": "86400",
};

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json", "Cache-Control": "no-store", ...CORS } });
}

async function body(req: Request): Promise<Record<string, unknown>> {
  const text = await req.text();
  if (text.length > 4096) throw new HttpError(413, "too_large");
  try {
    const v = JSON.parse(text || "{}");
    return v && typeof v === "object" ? v : {};
  } catch {
    throw new HttpError(400, "bad_json");
  }
}

function country(req: Request): string | null {
  return ((req as unknown as { cf?: { country?: string } }).cf?.country ?? req.headers.get("cf-ipcountry")) || null;
}

export function createHandler(d: AppDeps): (req: Request) => Promise<Response> {
  const product = d.env.PRODUCT_NAME ?? "Baguette";
  const mailer = d.env.RESEND_API_KEY && d.env.MAIL_FROM ? resendMailer(d.env.RESEND_API_KEY, d.env.MAIL_FROM, product) : null;
  const service = new Service({
    store: d.store,
    sign: d.sign,
    encSecret: d.env.KEY_ENC_SECRET,
    now: d.now,
    subscriptionMode: d.env.SUBSCRIPTION_MODE !== "off",
    flagsMessage: d.env.FLAGS_MESSAGE ?? null,
    mailer,
  });

  async function fulfilFromStripe(sessionId: string) {
    const session = await d.stripe("GET", `checkout/sessions/${encodeURIComponent(sessionId)}`);
    const sub = typeof session.subscription === "string" ? await d.stripe("GET", `subscriptions/${encodeURIComponent(session.subscription)}`) : null;
    return service.fulfil(session, sub);
  }

  async function checkout(offer: Offer, language: string, zone: 1 | 2 | 3 | 4): Promise<Response> {
    if (offer === "lifetime" && (await service.lifetimeLeft()) <= 0) return Response.redirect(`${d.env.SITE_URL}/${language}/?sold_out=1#pricing`, 303);
    const price = priceId(d.env.STRIPE_PRICES, offer, zone);
    if (!price) throw new HttpError(503, "not_configured");
    const founder = d.env.FOUNDER_COUPON && offer !== "lifetime" && d.now() < Number(d.env.FOUNDER_UNTIL ?? 0);
    const session = await d.stripe("POST", "checkout/sessions", {
      mode: offer === "lifetime" ? "payment" : "subscription",
      line_items: [{ price, quantity: 1 }],
      managed_payments: { enabled: true },
      success_url: `${d.env.API_BASE}/v1/success?session_id={CHECKOUT_SESSION_ID}&lang=${language}`,
      cancel_url: `${d.env.SITE_URL}/${language}/#pricing`,
      locale: checkoutLocale(language),
      metadata: { offer, zone: String(zone) },
      ...(founder ? { discounts: [{ coupon: d.env.FOUNDER_COUPON }] } : { allow_promotion_codes: true }),
    });
    return Response.redirect(String(session.url), 303);
  }

  async function webhook(req: Request): Promise<Response> {
    const payload = await req.text();
    if (!(await verifyWebhook(payload, req.headers.get("stripe-signature"), d.env.STRIPE_WEBHOOK_SECRET, d.now()))) return json({ error: "bad_signature" }, 400);
    const event = JSON.parse(payload) as { id: string; type: string; data: { object: Record<string, any> } };
    const obj = event.data?.object ?? {};
    switch (event.type) {
      case "checkout.session.completed":
      case "checkout.session.async_payment_succeeded":
        if (obj.payment_status === "paid" || obj.payment_status === "no_payment_required") await fulfilFromStripe(obj.id);
        break;
      case "customer.subscription.created":
      case "customer.subscription.updated":
      case "customer.subscription.deleted":
        await service.onSubscription(obj);
        break;
      case "charge.refunded": {
        // A refunded subscription must not renew: cancel it at Stripe right away.
        const revoked = await service.onRefund(obj);
        if (revoked?.subscription) await d.stripe("DELETE", `subscriptions/${encodeURIComponent(revoked.subscription)}`).catch(() => undefined);
        break;
      }
    }
    await d.store.markEvent(event.id, d.now());
    return json({ received: true });
  }

  return async (req) => {
    const url = new URL(req.url);
    const p = url.pathname;
    try {
      if (req.method === "OPTIONS") return new Response(null, { status: 204, headers: CORS });
      if (req.method === "GET" && p === "/v1/flags") return json({ token: await service.flags() });
      if (req.method === "GET" && p === "/v1/prices") {
        const zone = zoneOf(country(req));
        return json({ zone, currency: "EUR", prices: PRICES[zone], lifetime_left: await service.lifetimeLeft(), lifetime_cap: LIFETIME_CAP });
      }
      if (req.method === "POST" && p === "/v1/trial") return json(await service.trial((await body(req)).device));
      if (req.method === "POST" && p === "/v1/activate") {
        const b = await body(req);
        return json(await service.activate(b.key, b.device));
      }
      if (req.method === "POST" && p === "/v1/deactivate") {
        const b = await body(req);
        await service.deactivate(b.key, b.device);
        return json({ ok: true });
      }
      if (req.method === "POST" && p === "/v1/recover") {
        const b = await body(req);
        await service.recover(b.email, lang(typeof b.lang === "string" ? b.lang : null));
        return json({ ok: true }, 202);
      }
      if (req.method === "GET" && p === "/v1/checkout") {
        const offer = url.searchParams.get("offer");
        if (!isOffer(offer)) throw new HttpError(400, "bad_offer");
        return checkout(offer, lang(url.searchParams.get("lang")), zoneOf(country(req)));
      }
      if (req.method === "GET" && p === "/v1/success") {
        const language = lang(url.searchParams.get("lang"));
        const id = url.searchParams.get("session_id") ?? "";
        let key: string | null = null;
        if (/^cs_(test|live)_[A-Za-z0-9]{10,200}$/.test(id)) key = (await fulfilFromStripe(id).catch(() => null))?.key ?? null;
        return new Response(successPage(language, product, key, mailer !== null), {
          status: key ? 200 : 404,
          headers: { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store", "Referrer-Policy": "no-referrer", "Content-Security-Policy": "default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'" },
        });
      }
      if (req.method === "POST" && p === "/v1/stripe/webhook") return webhook(req);
      return json({ error: "not_found" }, 404);
    } catch (e) {
      if (e instanceof HttpError) return json({ error: e.code }, e.status);
      return json({ error: "server" }, 500);
    }
  };
}

let signer: { jwk: string; sign: Promise<Signer> } | null = null;

export default {
  async fetch(req: Request, env: Env): Promise<Response> {
    if (!signer || signer.jwk !== env.SIGNING_KEY) signer = { jwk: env.SIGNING_KEY, sign: signerFromJwk(env.SIGNING_KEY) };
    const handler = createHandler({
      env,
      store: new D1Store(env.DB),
      sign: await signer.sign,
      stripe: stripeApi(env.STRIPE_SECRET_KEY, env.STRIPE_API_VERSION),
      now: () => Math.floor(Date.now() / 1000),
    });
    return handler(req);
  },
};
