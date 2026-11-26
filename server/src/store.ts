// Storage: D1 (SQLite at Cloudflare) in production, an in-memory copy for the tests.
import type { LicenceKind } from "./keys.ts";

export type LicenceStatus = "active" | "past_due" | "canceled" | "revoked";

export interface Licence {
  id: string;
  key_hash: string;
  key_enc: string;
  kind: LicenceKind;
  status: LicenceStatus;
  /** End of the paid period (subscriptions), unix seconds. */
  period_end: number | null;
  customer: string | null;
  subscription: string | null;
  payment_intent: string | null;
  session: string;
  email_hash: string | null;
  created: number;
}

export interface Store {
  licenceById(id: string): Promise<Licence | null>;
  licenceBySession(session: string): Promise<Licence | null>;
  licenceByKeyHash(hash: string): Promise<Licence | null>;
  licenceBySubscription(sub: string): Promise<Licence | null>;
  licenceByPaymentIntent(pi: string): Promise<Licence | null>;
  licencesByCustomer(customer: string): Promise<Licence[]>;
  licencesByEmailHash(hash: string): Promise<Licence[]>;
  /** False when a licence already exists for this checkout session. */
  insertLicence(l: Licence): Promise<boolean>;
  updateLicence(id: string, patch: Partial<Pick<Licence, "status" | "period_end" | "subscription" | "customer">>): Promise<void>;
  countLifetime(): Promise<number>;
  activations(licenceId: string): Promise<{ device: string; last_seen: number }[]>;
  touchActivation(licenceId: string, device: string, now: number): Promise<void>;
  removeActivation(licenceId: string, device: string): Promise<void>;
  trialStart(device: string): Promise<number | null>;
  insertTrial(device: string, now: number): Promise<void>;
  /** Webhook idempotency: false when the event was already handled. */
  markEvent(id: string, now: number): Promise<boolean>;
}

export class MemoryStore implements Store {
  licences: Licence[] = [];
  acts = new Map<string, { device: string; first_seen: number; last_seen: number }[]>();
  trials = new Map<string, number>();
  events = new Set<string>();

  async licenceById(id: string) {
    return this.licences.find((l) => l.id === id) ?? null;
  }
  async licenceBySession(s: string) {
    return this.licences.find((l) => l.session === s) ?? null;
  }
  async licenceByKeyHash(h: string) {
    return this.licences.find((l) => l.key_hash === h) ?? null;
  }
  async licenceBySubscription(s: string) {
    return this.licences.find((l) => l.subscription === s) ?? null;
  }
  async licenceByPaymentIntent(p: string) {
    return this.licences.find((l) => l.payment_intent === p) ?? null;
  }
  async licencesByCustomer(c: string) {
    return this.licences.filter((l) => l.customer === c);
  }
  async licencesByEmailHash(h: string) {
    return this.licences.filter((l) => l.email_hash === h);
  }
  async insertLicence(l: Licence) {
    if (this.licences.some((x) => x.session === l.session || x.key_hash === l.key_hash)) return false;
    this.licences.push({ ...l });
    return true;
  }
  async updateLicence(id: string, patch: Partial<Licence>) {
    const l = this.licences.find((x) => x.id === id);
    if (l) Object.assign(l, patch);
  }
  async countLifetime() {
    return this.licences.filter((l) => l.kind === "lifetime" && l.status !== "revoked").length;
  }
  async activations(id: string) {
    return (this.acts.get(id) ?? []).map(({ device, last_seen }) => ({ device, last_seen }));
  }
  async touchActivation(id: string, device: string, now: number) {
    const list = this.acts.get(id) ?? [];
    const a = list.find((x) => x.device === device);
    if (a) a.last_seen = now;
    else list.push({ device, first_seen: now, last_seen: now });
    this.acts.set(id, list);
  }
  async removeActivation(id: string, device: string) {
    this.acts.set(id, (this.acts.get(id) ?? []).filter((x) => x.device !== device));
  }
  async trialStart(device: string) {
    return this.trials.get(device) ?? null;
  }
  async insertTrial(device: string, now: number) {
    if (!this.trials.has(device)) this.trials.set(device, now);
  }
  async markEvent(id: string) {
    if (this.events.has(id)) return false;
    this.events.add(id);
    return true;
  }
}

/** The subset of the D1 API used here (avoids a dependency on the Workers type package). */
export interface D1Like {
  prepare(sql: string): {
    bind(...values: unknown[]): {
      first<T>(): Promise<T | null>;
      all<T>(): Promise<{ results: T[] }>;
      run(): Promise<{ meta?: { changes?: number } }>;
    };
  };
}

export class D1Store implements Store {
  private db: D1Like;
  constructor(db: D1Like) {
    this.db = db;
  }

  private one(sql: string, ...v: unknown[]) {
    return this.db.prepare(sql).bind(...v).first<Licence>();
  }
  private many(sql: string, ...v: unknown[]) {
    return this.db.prepare(sql).bind(...v).all<Licence>().then((r) => r.results);
  }

  licenceById(id: string) {
    return this.one("SELECT * FROM licences WHERE id = ?1", id);
  }
  licenceBySession(s: string) {
    return this.one("SELECT * FROM licences WHERE session = ?1", s);
  }
  licenceByKeyHash(h: string) {
    return this.one("SELECT * FROM licences WHERE key_hash = ?1", h);
  }
  licenceBySubscription(s: string) {
    return this.one("SELECT * FROM licences WHERE subscription = ?1", s);
  }
  licenceByPaymentIntent(p: string) {
    return this.one("SELECT * FROM licences WHERE payment_intent = ?1", p);
  }
  licencesByCustomer(c: string) {
    return this.many("SELECT * FROM licences WHERE customer = ?1", c);
  }
  licencesByEmailHash(h: string) {
    return this.many("SELECT * FROM licences WHERE email_hash = ?1", h);
  }
  async insertLicence(l: Licence) {
    const r = await this.db
      .prepare(
        `INSERT OR IGNORE INTO licences (id, key_hash, key_enc, kind, status, period_end, customer, subscription, payment_intent, session, email_hash, created)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)`,
      )
      .bind(l.id, l.key_hash, l.key_enc, l.kind, l.status, l.period_end, l.customer, l.subscription, l.payment_intent, l.session, l.email_hash, l.created)
      .run();
    return (r.meta?.changes ?? 0) > 0;
  }
  async updateLicence(id: string, patch: Partial<Licence>) {
    const cols = (["status", "period_end", "subscription", "customer"] as const).filter((c) => c in patch);
    if (!cols.length) return;
    const sets = cols.map((c, i) => `${c} = ?${i + 2}`).join(", ");
    await this.db.prepare(`UPDATE licences SET ${sets} WHERE id = ?1`).bind(id, ...cols.map((c) => patch[c] ?? null)).run();
  }
  async countLifetime() {
    const r = await this.db.prepare("SELECT COUNT(*) AS n FROM licences WHERE kind = 'lifetime' AND status != 'revoked'").bind().first<{ n: number }>();
    return r?.n ?? 0;
  }
  async activations(id: string) {
    const r = await this.db.prepare("SELECT device, last_seen FROM activations WHERE licence_id = ?1").bind(id).all<{ device: string; last_seen: number }>();
    return r.results;
  }
  async touchActivation(id: string, device: string, now: number) {
    await this.db
      .prepare("INSERT INTO activations (licence_id, device, first_seen, last_seen) VALUES (?1, ?2, ?3, ?3) ON CONFLICT(licence_id, device) DO UPDATE SET last_seen = excluded.last_seen")
      .bind(id, device, now)
      .run();
  }
  async removeActivation(id: string, device: string) {
    await this.db.prepare("DELETE FROM activations WHERE licence_id = ?1 AND device = ?2").bind(id, device).run();
  }
  async trialStart(device: string) {
    const r = await this.db.prepare("SELECT started FROM trials WHERE device = ?1").bind(device).first<{ started: number }>();
    return r?.started ?? null;
  }
  async insertTrial(device: string, now: number) {
    await this.db.prepare("INSERT OR IGNORE INTO trials (device, started) VALUES (?1, ?2)").bind(device, now).run();
  }
  async markEvent(id: string, now: number) {
    const r = await this.db.prepare("INSERT OR IGNORE INTO events (id, created) VALUES (?1, ?2)").bind(id, now).run();
    return (r.meta?.changes ?? 0) > 0;
  }
}
