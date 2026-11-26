import type { Settings } from "./types";

/** Money in the user's currency (quotes are computed in USD at list price). */
export function money(usd: number, s: Pick<Settings, "currency" | "usd_to_eur">, lang: string): string {
  const eur = s.currency === "eur";
  const value = eur ? usd * s.usd_to_eur : usd;
  return new Intl.NumberFormat(locale(lang), {
    style: "currency",
    currency: eur ? "EUR" : "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(value);
}

/** An amount already in the display currency (freelance prices). */
export function amount(value: number, currency: "eur" | "usd", lang: string): string {
  return new Intl.NumberFormat(locale(lang), { style: "currency", currency: currency === "eur" ? "EUR" : "USD", minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(value);
}

/** Minutes as "1 h 05" or "45 min". */
export function minutes(total: number, lang: string): string {
  const fmt = new Intl.NumberFormat(locale(lang));
  const h = Math.floor(total / 60);
  const m = Math.round(total % 60);
  return h > 0 ? `${fmt.format(h)} h ${String(m).padStart(2, "0")}` : `${fmt.format(m)} min`;
}

/** Share of the subscription window, e.g. "11 %". */
export function percent(pct: number, lang: string): string {
  return new Intl.NumberFormat(locale(lang), {
    style: "percent",
    maximumFractionDigits: pct < 10 ? 1 : 0,
  }).format(Math.max(0, pct) / 100);
}

export function duration(ms: number, lang: string): string {
  const s = Math.round(ms / 1000);
  const fmt = new Intl.NumberFormat(locale(lang));
  return s < 60 ? `${fmt.format(s)} s` : `${fmt.format(Math.floor(s / 60))} min ${fmt.format(s % 60)} s`;
}

export function date(ts: number, lang: string): string {
  return new Intl.DateTimeFormat(locale(lang), { dateStyle: "medium", timeStyle: "short" }).format(new Date(ts * 1000));
}

/** Day only, e.g. "25 nov. 2026". */
export function day(ts: number, lang: string): string {
  return new Intl.DateTimeFormat(locale(lang), { dateStyle: "medium" }).format(new Date(ts * 1000));
}

/** Clock time, e.g. "07:00". */
export function clock(ts: number, lang: string): string {
  return new Intl.DateTimeFormat(locale(lang), { hour: "2-digit", minute: "2-digit" }).format(new Date(ts * 1000));
}

/** Next time the clock shows "HH:MM" after `nowMs` (today or tomorrow), in unix seconds. */
export function nextAt(hhmm: string, nowMs: number = Date.now()): number | null {
  const m = /^(\d{1,2}):(\d{2})$/.exec(hhmm.trim());
  if (!m) return null;
  const [h, min] = [Number(m[1]), Number(m[2])];
  if (h > 23 || min > 59) return null;
  const d = new Date(nowMs);
  d.setHours(h, min, 0, 0);
  if (d.getTime() <= nowMs) d.setDate(d.getDate() + 1);
  return Math.floor(d.getTime() / 1000);
}

export function locale(lang: string): string {
  return lang === "pt" ? "pt-BR" : lang;
}

export const RTL_LANGUAGES = new Set(["ar"]);

export function direction(lang: string): "rtl" | "ltr" {
  return RTL_LANGUAGES.has(lang) ? "rtl" : "ltr";
}
