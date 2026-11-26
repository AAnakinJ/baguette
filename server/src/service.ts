// Licence rules: trials, activation on up to 3 computers, subscription state from Stripe,
// refunds, and the signed tokens the app checks offline.
import { DEVICE_RE, KEY_RE, decryptKey, encryptKey, hashEmail, hashKey, newLicenceKey, normalizeKey, type LicenceKind } from "./keys.ts";
import { LIFETIME_CAP } from "./pricing.ts";
import { periodEnd } from "./stripe.ts";
import type { Licence, LicenceStatus, Store } from "./store.ts";
import type { Signer } from "./token.ts";

const DAY = 86_400;
export const TRIAL_DAYS = 14;
export const MAX_DEVICES = 3;
/** A Pro token lasts at most this long: the app comes back for a fresh one regularly. */
const PRO_TOKEN_DAYS = 35;
const PRO_GRACE_DAYS = 3;
const LIFETIME_TOKEN_DAYS = 90;
const FLAGS_DAYS = 7;

export class HttpError extends Error {
  status: number;
  code: string;
  constructor(status: number, code: string) {
    super(code);
    this.status = status;
    this.code = code;
  }
}

export interface Mailer {
  sendKeys(to: string, keys: string[], lang: string): Promise<void>;
}

export interface Deps {
  store: Store;
  sign: Signer;
  encSecret: string;
  now: () => number;
  subscriptionMode: boolean;
  flagsMessage?: string | null;
  mailer?: Mailer | null;
}

export interface Issued {
  token: string;
  kind: "trial" | LicenceKind;
  exp: number;
}

function checkDevice(device: unknown): string {
  if (typeof device !== "string" || !DEVICE_RE.test(device)) throw new HttpError(400, "bad_device");
  return device;
}

function checkKey(key: unknown): string {
  if (typeof key !== "string") throw new HttpError(400, "bad_key");
  const k = normalizeKey(key);
  if (!KEY_RE.test(k)) throw new HttpError(400, "bad_key");
  return k;
}

export class Service {
  private d: Deps;
  constructor(d: Deps) {
    this.d = d;
  }

  private async issue(kind: Issued["kind"], lid: string, dev: string, exp: number): Promise<Issued> {
    const iat = this.d.now();
    const token = await this.d.sign({ v: 1, typ: "licence", kind, lid, dev, iat, exp });
    return { token, kind, exp };
  }

  /** One 14-day trial per computer, whatever the number of reinstalls. */
  async trial(deviceIn: unknown): Promise<Issued> {
    const device = checkDevice(deviceIn);
    const now = this.d.now();
    let started = await this.d.store.trialStart(device);
    if (started === null) {
      await this.d.store.insertTrial(device, now);
      started = (await this.d.store.trialStart(device)) ?? now;
    }
    const exp = started + TRIAL_DAYS * DAY;
    if (exp <= now) throw new HttpError(409, "trial_used");
    return this.issue("trial", `trial_${device.slice(0, 8)}`, device, exp);
  }

  private tokenExp(l: Licence, now: number): number {
    if (l.kind === "lifetime") return now + LIFETIME_TOKEN_DAYS * DAY;
    const cap = now + PRO_TOKEN_DAYS * DAY;
    // Period not known yet (webhook on its way): a short token, refreshed soon.
    if (l.period_end === null) return now + PRO_GRACE_DAYS * DAY;
    return Math.min(l.period_end + PRO_GRACE_DAYS * DAY, cap);
  }

  private usable(l: Licence | null, now: number): Licence {
    if (!l) throw new HttpError(404, "unknown_key");
    if (l.status === "revoked") throw new HttpError(403, "revoked");
    if (l.kind === "pro" && l.status !== "active" && (l.period_end === null || l.period_end + PRO_GRACE_DAYS * DAY <= now)) throw new HttpError(403, "expired");
    return l;
  }

  /** Activates (or refreshes) a key on a computer. */
  async activate(keyIn: unknown, deviceIn: unknown): Promise<Issued> {
    const key = checkKey(keyIn);
    const device = checkDevice(deviceIn);
    const now = this.d.now();
    const l = this.usable(await this.d.store.licenceByKeyHash(await hashKey(key)), now);
    const acts = await this.d.store.activations(l.id);
    if (!acts.some((a) => a.device === device) && acts.length >= MAX_DEVICES) throw new HttpError(409, "too_many_devices");
    await this.d.store.touchActivation(l.id, device, now);
    const exp = this.tokenExp(l, now);
    if (exp <= now) throw new HttpError(403, "expired");
    return this.issue(l.kind, l.id, device, exp);
  }

  /** Frees a seat (moving to a new computer). */
  async deactivate(keyIn: unknown, deviceIn: unknown): Promise<void> {
    const key = checkKey(keyIn);
    const device = checkDevice(deviceIn);
    const l = await this.d.store.licenceByKeyHash(await hashKey(key));
    if (!l) throw new HttpError(404, "unknown_key");
    await this.d.store.removeActivation(l.id, device);
  }

  /** Remote switches (Phase 1: the subscription mode can be turned off without an update). */
  async flags(): Promise<string> {
    const iat = this.d.now();
    return this.d.sign({ v: 1, typ: "flags", iat, exp: iat + FLAGS_DAYS * DAY, subscription_mode: this.d.subscriptionMode, message: this.d.flagsMessage ?? null });
  }

  async lifetimeLeft(): Promise<number> {
    return Math.max(0, LIFETIME_CAP - (await this.d.store.countLifetime()));
  }

  /**
   * Creates the licence for a paid Checkout Session, once (success page and webhook may both
   * arrive). Returns the key in clear only to show it to the buyer.
   */
  async fulfil(session: Record<string, any>, subscription?: Record<string, any> | null): Promise<{ licence: Licence; key: string; created: boolean }> {
    if (!session?.id || !["paid", "no_payment_required"].includes(session.payment_status)) throw new HttpError(402, "not_paid");
    const existing = await this.d.store.licenceBySession(session.id);
    if (existing) return { licence: existing, key: await decryptKey(existing.key_enc, this.d.encSecret), created: false };
    const offer = session.metadata?.offer;
    const kind: LicenceKind = offer === "lifetime" || session.mode === "payment" ? "lifetime" : "pro";
    const key = newLicenceKey(kind);
    const email: string | undefined = session.customer_details?.email ?? session.customer_email ?? undefined;
    const subId = typeof session.subscription === "string" ? session.subscription : (session.subscription?.id ?? null);
    const licence: Licence = {
      id: `lic_${crypto.randomUUID().replace(/-/g, "").slice(0, 20)}`,
      key_hash: await hashKey(key),
      key_enc: await encryptKey(key, this.d.encSecret),
      kind,
      status: "active",
      period_end: subscription ? periodEnd(subscription) : null,
      customer: typeof session.customer === "string" ? session.customer : (session.customer?.id ?? null),
      subscription: subId,
      payment_intent: typeof session.payment_intent === "string" ? session.payment_intent : (session.payment_intent?.id ?? null),
      session: session.id,
      email_hash: email ? await hashEmail(email) : null,
      created: this.d.now(),
    };
    if (!(await this.d.store.insertLicence(licence))) {
      const raced = await this.d.store.licenceBySession(session.id);
      if (!raced) throw new HttpError(500, "store");
      return { licence: raced, key: await decryptKey(raced.key_enc, this.d.encSecret), created: false };
    }
    if (email && this.d.mailer) await this.d.mailer.sendKeys(email, [key], session.locale ?? "en").catch(() => undefined);
    return { licence, key, created: true };
  }

  /** customer.subscription.created / updated / deleted */
  async onSubscription(sub: Record<string, any>): Promise<void> {
    const l = await this.d.store.licenceBySubscription(sub.id);
    if (!l || l.status === "revoked") return;
    const map: Record<string, LicenceStatus> = { active: "active", trialing: "active", past_due: "past_due", unpaid: "past_due", canceled: "canceled", incomplete_expired: "canceled", paused: "canceled" };
    const status = map[sub.status] ?? l.status;
    const end = periodEnd(sub);
    await this.d.store.updateLicence(l.id, { status, period_end: end ?? l.period_end });
  }

  /**
   * charge.refunded: a full refund ends the licence (14-day guarantee, or Stripe's own refunds).
   * Returns the revoked licence, so its subscription can be cancelled (otherwise it would renew).
   */
  async onRefund(charge: Record<string, any>): Promise<Licence | null> {
    if (!charge.refunded) return null;
    const pi = typeof charge.payment_intent === "string" ? charge.payment_intent : charge.payment_intent?.id;
    let l = pi ? await this.d.store.licenceByPaymentIntent(pi) : null;
    const customer = typeof charge.customer === "string" ? charge.customer : charge.customer?.id;
    if (!l && customer) {
      const all = (await this.d.store.licencesByCustomer(customer)).filter((x) => x.status !== "revoked");
      if (all.length === 1) l = all[0];
    }
    if (!l) return null;
    await this.d.store.updateLicence(l.id, { status: "revoked" });
    return l;
  }

  /** Sends the keys bought with this address. Same answer whether or not it matches. */
  async recover(emailIn: unknown, lang: string): Promise<void> {
    if (typeof emailIn !== "string" || !/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(emailIn.trim()) || emailIn.length > 254) throw new HttpError(400, "bad_email");
    // No email service yet (it needs a domain): say so instead of pretending to send.
    if (!this.d.mailer) throw new HttpError(503, "mail_unavailable");
    const list = (await this.d.store.licencesByEmailHash(await hashEmail(emailIn))).filter((l) => l.status !== "revoked");
    if (!list.length) return;
    const keys = await Promise.all(list.map((l) => decryptKey(l.key_enc, this.d.encSecret)));
    await this.d.mailer.sendKeys(emailIn.trim(), keys, lang).catch(() => undefined);
  }
}
