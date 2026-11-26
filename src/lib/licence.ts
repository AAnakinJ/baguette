// Talks to the licence server. Every token it returns is checked by the Rust side with the
// public key built into the app, so this file holds no secret and trusts nothing it receives.
import { api, inTauri } from "./api";
import { mock } from "./mock";
import type { LicenceView } from "./types";

type Res = { status: number; json: () => Promise<any> };
const http = (url: string, init?: RequestInit): Promise<Res> => (inTauri ? fetch(url, init) : mock.http(url, init));

/** Tokens ending within 20 days are renewed at start-up (Pro tokens last up to 35 days). */
const REFRESH_BEFORE = 20 * 86_400;
const TIMEOUT_MS = 8_000;

async function post(base: string, path: string, body: unknown): Promise<{ status: number; data: any }> {
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), TIMEOUT_MS);
  try {
    const res = await http(`${base}${path}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body), signal: ctl.signal });
    return { status: res.status, data: await res.json().catch(() => ({})) };
  } finally {
    clearTimeout(timer);
  }
}

export class LicenceError extends Error {
  constructor(public code: string) {
    super(code);
  }
}

/**
 * Start-up: remote flags, then the trial or the renewal of a Pro token. Quiet when offline:
 * the app keeps working with what it has.
 */
export async function syncLicence(view: LicenceView): Promise<LicenceView> {
  // Early access: nothing to check and nothing sent anywhere.
  if (view.early_access) return view;
  const base = view.api_base;
  try {
    const res = await http(`${base}/v1/flags`);
    if (res.status === 200) await api.flagsApply((await res.json()).token);
  } catch {
    /* offline or flags refused: keep the last ones */
  }
  try {
    const r = await api.licenceRefreshRequest();
    const now = Date.now() / 1000;
    if (r.key) {
      if (r.token_exp === null || r.token_exp - now < REFRESH_BEFORE) {
        const { status, data } = await post(base, "/v1/activate", { key: r.key, device: r.device });
        if (status === 200) return await api.licenceInstall(data.token, null);
        // Refunded, cancelled and ended, or unknown: Pro stops; the key stays visible.
        if (status === 403 || status === 404) return await api.licenceForget(true);
      }
    } else if (r.token_exp === null && !view.status.trial_refused) {
      const { status, data } = await post(base, "/v1/trial", { device: r.device });
      if (status === 200) return await api.licenceInstall(data.token, null);
      if (status === 409) return await api.licenceTrialRefused();
    }
  } catch {
    /* offline */
  }
  return view;
}

export async function activateKey(view: LicenceView, key: string): Promise<LicenceView> {
  let res: { status: number; data: any };
  try {
    res = await post(view.api_base, "/v1/activate", { key: key.trim().toUpperCase(), device: view.device });
  } catch {
    throw new LicenceError("offline");
  }
  if (res.status !== 200) throw new LicenceError(typeof res.data?.error === "string" ? res.data.error : "server");
  return api.licenceInstall(res.data.token, key.trim().toUpperCase());
}

/** Frees this computer's seat, then removes the licence locally. */
export async function deactivate(view: LicenceView): Promise<LicenceView> {
  const r = await api.licenceRefreshRequest();
  if (r.key) {
    try {
      await post(view.api_base, "/v1/deactivate", { key: r.key, device: r.device });
    } catch {
      throw new LicenceError("offline");
    }
  }
  return api.licenceForget(false);
}

export async function recoverKeys(view: LicenceView, email: string, lang: string): Promise<void> {
  let res: { status: number; data: any };
  try {
    res = await post(view.api_base, "/v1/recover", { email: email.trim(), lang });
  } catch {
    throw new LicenceError("offline");
  }
  if (res.status >= 400) throw new LicenceError(res.data?.error ?? "server");
}

export const pricingUrl = (view: LicenceView, lang: string) => `${view.site_url}/${lang}/#pricing`;
