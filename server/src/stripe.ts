// Stripe: webhook signature check and the few API calls we need (no SDK, plain fetch).
// Managed Payments: Stripe is the merchant of record (taxes, disputes, refunds).

export const STRIPE_API_VERSION = "2025-03-31.basil";

function hex(bytes: ArrayBuffer): string {
  return Array.from(new Uint8Array(bytes), (b) => b.toString(16).padStart(2, "0")).join("");
}

function safeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let d = 0;
  for (let i = 0; i < a.length; i++) d |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return d === 0;
}

/** Checks the `Stripe-Signature` header (HMAC-SHA256 of "t.payload", 5 minutes tolerance). */
export async function verifyWebhook(payload: string, header: string | null, secret: string, now: number, tolerance = 300): Promise<boolean> {
  if (!header || !secret) return false;
  const parts = header.split(",").map((p) => p.trim().split("=") as [string, string]);
  const t = Number(parts.find(([k]) => k === "t")?.[1]);
  const sigs = parts.filter(([k]) => k === "v1").map(([, v]) => v);
  if (!Number.isFinite(t) || !sigs.length || Math.abs(now - t) > tolerance) return false;
  const key = await crypto.subtle.importKey("raw", new TextEncoder().encode(secret), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  const expected = hex(await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(`${t}.${payload}`)));
  return sigs.some((s) => safeEqual(s, expected));
}

/** Form-encodes nested params the way Stripe expects (a[b][0]=c). */
export function formEncode(params: Record<string, unknown>, prefix = ""): string[] {
  const out: string[] = [];
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined || v === null) continue;
    const key = prefix ? `${prefix}[${k}]` : k;
    if (Array.isArray(v)) v.forEach((item, i) => out.push(...(typeof item === "object" ? formEncode(item as Record<string, unknown>, `${key}[${i}]`) : [`${encodeURIComponent(`${key}[${i}]`)}=${encodeURIComponent(String(item))}`])));
    else if (typeof v === "object") out.push(...formEncode(v as Record<string, unknown>, key));
    else out.push(`${encodeURIComponent(key)}=${encodeURIComponent(String(v))}`);
  }
  return out;
}

export type StripeApi = (method: "GET" | "POST" | "DELETE", path: string, params?: Record<string, unknown>) => Promise<Record<string, any>>;

export function stripeApi(secretKey: string, version = STRIPE_API_VERSION, fetcher: typeof fetch = fetch): StripeApi {
  return async (method, path, params) => {
    const body = params ? formEncode(params).join("&") : undefined;
    const url = `https://api.stripe.com/v1/${path}${method === "GET" && body ? `?${body}` : ""}`;
    const res = await fetcher(url, {
      method,
      headers: { Authorization: `Bearer ${secretKey}`, "Stripe-Version": version, ...(method === "POST" ? { "Content-Type": "application/x-www-form-urlencoded" } : {}) },
      body: method === "POST" ? body : undefined,
    });
    const json = (await res.json()) as Record<string, any>;
    if (!res.ok) throw new Error(`stripe ${res.status}: ${json?.error?.message ?? "error"}`);
    return json;
  };
}

/** Stripe Checkout languages we can pass; the others (ar, hi) let Stripe pick. */
const CHECKOUT_LOCALES: Record<string, string> = { en: "en", fr: "fr", es: "es", pt: "pt-BR", de: "de", ja: "ja", ko: "ko", id: "id" };
export const checkoutLocale = (lang: string) => CHECKOUT_LOCALES[lang] ?? "auto";

/** Period end of a subscription: on the item since API 2025-03-31, on the subscription before. */
export function periodEnd(sub: Record<string, any>): number | null {
  return sub.current_period_end ?? sub.items?.data?.[0]?.current_period_end ?? null;
}
