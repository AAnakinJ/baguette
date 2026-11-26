// The production SQL (D1Store + schema.sql) on a real SQLite database.
import { describe, expect, it } from "vitest";
import { nodeD1 } from "../scripts/node-d1.mjs";
import { HttpError, Service } from "../src/service.ts";
import { D1Store, type Licence } from "../src/store.ts";
import { DEVICE, DEVICE2, ENC_SECRET, testSigner } from "./helpers.ts";

const DAY = 86_400;

async function code(p: Promise<unknown>) {
  try {
    await p;
    return "ok";
  } catch (e) {
    return e instanceof HttpError ? e.code : String(e);
  }
}

describe("D1Store on SQLite", () => {
  it("runs the whole licence life cycle with the real schema", async () => {
    const store = new D1Store(nodeD1());
    let now = 1_800_000_000;
    const service = new Service({ store, sign: await testSigner(), encSecret: ENC_SECRET, now: () => now, subscriptionMode: true });

    // Trial once per device.
    expect((await service.trial(DEVICE)).kind).toBe("trial");
    now += 15 * DAY;
    expect(await code(service.trial(DEVICE))).toBe("trial_used");

    // Purchase, idempotent, then subscription updates.
    const session = { id: "cs_test_1", mode: "subscription", payment_status: "paid", customer: "cus_1", subscription: "sub_1", metadata: { offer: "monthly" }, customer_details: { email: "a@b.co" } };
    const { key, created } = await service.fulfil(session, { id: "sub_1", items: { data: [{ current_period_end: now + 30 * DAY }] } });
    expect(created).toBe(true);
    expect((await service.fulfil(session)).created).toBe(false);
    expect(await store.licencesByCustomer("cus_1")).toHaveLength(1);

    for (const d of [DEVICE, DEVICE2, "c".repeat(32)]) expect(await code(service.activate(key, d))).toBe("ok");
    expect(await code(service.activate(key, "d".repeat(32)))).toBe("too_many_devices");
    await service.activate(key, DEVICE); // refresh: same seat, no new one
    expect(await store.activations((await store.licenceBySession("cs_test_1"))!.id)).toHaveLength(3);
    await service.deactivate(key, DEVICE2);
    expect(await code(service.activate(key, "d".repeat(32)))).toBe("ok");

    await service.onSubscription({ id: "sub_1", status: "past_due", items: { data: [{ current_period_end: now + 2 * DAY }] } });
    const l = (await store.licenceBySubscription("sub_1")) as Licence;
    expect([l.status, l.period_end]).toEqual(["past_due", now + 2 * DAY]);

    // Lifetime purchase, counted, then refunded.
    const life = await service.fulfil({ id: "cs_test_2", mode: "payment", payment_status: "paid", payment_intent: "pi_2", customer: "cus_2", metadata: { offer: "lifetime" } });
    expect(await service.lifetimeLeft()).toBe(499);
    await service.onRefund({ payment_intent: "pi_2", refunded: true });
    expect(await code(service.activate(life.key, DEVICE))).toBe("revoked");
    expect(await service.lifetimeLeft()).toBe(500);

    // Webhook idempotency table.
    expect(await store.markEvent("evt_1", now)).toBe(true);
    expect(await store.markEvent("evt_1", now)).toBe(false);
  });
});
