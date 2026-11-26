import { describe, expect, it } from "vitest";
import { hashEmail, hashKey, KEY_RE, newLicenceKey } from "../src/keys.ts";
import { HttpError, MAX_DEVICES, Service, type Mailer } from "../src/service.ts";
import { MemoryStore } from "../src/store.ts";
import { verifyToken } from "../src/token.ts";
import { DEVICE, DEVICE2, ENC_SECRET, TEST_KEY, testSigner } from "./helpers.ts";

const DAY = 86_400;

async function setup(start = 1_800_000_000) {
  const store = new MemoryStore();
  let now = start;
  const sent: { to: string; keys: string[] }[] = [];
  const mailer: Mailer = { sendKeys: async (to, keys) => void sent.push({ to, keys }) };
  const service = new Service({ store, sign: await testSigner(), encSecret: ENC_SECRET, now: () => now, subscriptionMode: true, mailer });
  return { store, service, sent, tick: (s: number) => (now += s), at: () => now };
}

const paid = (id: string, extra: Record<string, unknown> = {}) => ({
  id,
  mode: "subscription",
  payment_status: "paid",
  customer: "cus_1",
  subscription: "sub_1",
  customer_details: { email: "Buyer@Example.com" },
  metadata: { offer: "monthly" },
  ...extra,
});

async function code(p: Promise<unknown>): Promise<string> {
  try {
    await p;
    return "ok";
  } catch (e) {
    return e instanceof HttpError ? e.code : String(e);
  }
}

describe("keys", () => {
  it("are well formed and unique", () => {
    const a = newLicenceKey("pro");
    expect(a).toMatch(KEY_RE);
    expect(newLicenceKey("lifetime")).toMatch(/^LIFE-/);
    expect(new Set(Array.from({ length: 200 }, () => newLicenceKey("pro"))).size).toBe(200);
  });
});

describe("trial", () => {
  it("gives one 14-day trial per computer, even after a reinstall", async () => {
    const { service, tick, at } = await setup();
    const t = await service.trial(DEVICE);
    expect(t.kind).toBe("trial");
    expect(t.exp).toBe(at() + 14 * DAY);
    tick(3 * DAY);
    expect((await service.trial(DEVICE)).exp).toBe(t.exp);
    tick(11 * DAY);
    expect(await code(service.trial(DEVICE))).toBe("trial_used");
    expect(await code(service.trial("not-a-device"))).toBe("bad_device");
  });
});

describe("purchase and activation", () => {
  it("creates the licence once, emails it, and activates on up to 3 computers", async () => {
    const { service, store, sent } = await setup();
    const first = await service.fulfil(paid("cs_1"), { id: "sub_1", items: { data: [{ current_period_end: 1_800_000_000 + 30 * DAY }] } });
    expect(first.created).toBe(true);
    expect(first.key).toMatch(/^PRO-/);
    const again = await service.fulfil(paid("cs_1"));
    expect(again).toMatchObject({ created: false, key: first.key });
    expect(store.licences).toHaveLength(1);
    expect(sent).toEqual([{ to: "Buyer@Example.com", keys: [first.key] }]);
    // Nothing personal in clear: the key and the email are only stored hashed / encrypted.
    const row = JSON.stringify(store.licences[0]);
    expect(row).not.toContain(first.key);
    expect(row).not.toContain("xample");
    expect(store.licences[0].email_hash).toBe(await hashEmail("buyer@example.com"));

    const issued = await service.activate(first.key.toLowerCase(), DEVICE);
    const claims = await verifyToken(TEST_KEY.publicRaw, issued.token);
    expect(claims).toMatchObject({ typ: "licence", kind: "pro", dev: DEVICE, exp: 1_800_000_000 + 33 * DAY });
    const devices = ["a", "b", "c", "d"].map((c) => c.repeat(32));
    for (const d of devices.slice(0, MAX_DEVICES - 1)) await service.activate(first.key, d);
    expect(await code(service.activate(first.key, devices[3]))).toBe("too_many_devices");
    await service.deactivate(first.key, devices[0]);
    expect(await code(service.activate(first.key, devices[3]))).toBe("ok");
    expect(await code(service.activate("PRO-2222-2222-2222-2222", DEVICE))).toBe("unknown_key");
    expect(await code(service.activate("hello", DEVICE))).toBe("bad_key");
  });

  it("refuses unpaid sessions", async () => {
    const { service } = await setup();
    expect(await code(service.fulfil(paid("cs_2", { payment_status: "unpaid" })))).toBe("not_paid");
  });

  it("follows the subscription: renewal, cancellation, end of period", async () => {
    const { service, tick, at } = await setup();
    const { key } = await service.fulfil(paid("cs_3"));
    // Period unknown yet: short token.
    expect((await service.activate(key, DEVICE)).exp).toBe(at() + 3 * DAY);
    await service.onSubscription({ id: "sub_1", status: "active", items: { data: [{ current_period_end: at() + 200 * DAY }] } });
    expect((await service.activate(key, DEVICE)).exp).toBe(at() + 35 * DAY);
    await service.onSubscription({ id: "sub_1", status: "canceled", items: { data: [{ current_period_end: at() + 10 * DAY }] } });
    expect((await service.activate(key, DEVICE)).exp).toBe(at() + 13 * DAY);
    tick(13 * DAY);
    expect(await code(service.activate(key, DEVICE))).toBe("expired");
  });

  it("a full refund ends the licence, a partial one does not", async () => {
    const { service } = await setup();
    const { key } = await service.fulfil(paid("cs_4", { mode: "payment", subscription: null, payment_intent: "pi_4", metadata: { offer: "lifetime" } }));
    expect(key).toMatch(/^LIFE-/);
    await service.onRefund({ payment_intent: "pi_4", refunded: false });
    expect(await code(service.activate(key, DEVICE))).toBe("ok");
    await service.onRefund({ payment_intent: "pi_4", refunded: true });
    expect(await code(service.activate(key, DEVICE))).toBe("revoked");
  });

  it("subscription refunds are found through the customer", async () => {
    const { service } = await setup();
    const { key } = await service.fulfil(paid("cs_5", { customer: "cus_5" }));
    await service.onRefund({ customer: "cus_5", refunded: true });
    expect(await code(service.activate(key, DEVICE2))).toBe("revoked");
  });

  it("lifetime seats are counted", async () => {
    const { service } = await setup();
    expect(await service.lifetimeLeft()).toBe(500);
    await service.fulfil(paid("cs_6", { mode: "payment", subscription: null, metadata: { offer: "lifetime" } }));
    expect(await service.lifetimeLeft()).toBe(499);
  });

  it("recovers keys by email without telling whether the address is known", async () => {
    const { service, sent } = await setup();
    const { key } = await service.fulfil(paid("cs_7", { customer_details: { email: "a@b.co" } }));
    sent.length = 0;
    await service.recover(" A@B.co ", "fr");
    await service.recover("nobody@b.co", "fr");
    expect(sent).toEqual([{ to: "A@B.co", keys: [key] }]);
    expect(await code(service.recover("not an email", "fr"))).toBe("bad_email");
  });

  it("says when no email service is configured", async () => {
    const service = new Service({ store: new MemoryStore(), sign: await testSigner(), encSecret: ENC_SECRET, now: () => 0, subscriptionMode: true, mailer: null });
    expect(await code(service.recover("a@b.co", "fr"))).toBe("mail_unavailable");
  });
});

describe("flags", () => {
  it("are signed and carry the subscription switch", async () => {
    const { service } = await setup();
    const claims = await verifyToken(TEST_KEY.publicRaw, await service.flags());
    expect(claims).toMatchObject({ typ: "flags", subscription_mode: true });
  });
});

describe("hashKey", () => {
  it("ignores case and spaces", async () => {
    expect(await hashKey(" pro-2345-6789-abcd-efgh ")).toBe(await hashKey("PRO-2345-6789-ABCD-EFGH"));
  });
});
