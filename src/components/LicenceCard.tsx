import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, openUrl } from "../lib/api";
import { activateKey, deactivate, LicenceError, pricingUrl, recoverKeys } from "../lib/licence";
import type { LicenceView } from "../lib/types";
import { day } from "../lib/format";
import { Card, Spinner } from "../lib/ui";

/** Settings → Licence: trial, Pro key, buying, moving to another computer, lost key. */
export function LicenceCard({ onChanged }: { onChanged: () => void }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const [view, setView] = useState<LicenceView | null>(null);
  const [key, setKey] = useState("");
  const [email, setEmail] = useState("");
  const [busy, setBusy] = useState<null | "activate" | "remove" | "recover">(null);
  const [msg, setMsg] = useState<{ tone: "ok" | "bad"; text: string } | null>(null);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [showRecover, setShowRecover] = useState(false);

  useEffect(() => {
    api.licenceGet().then(setView).catch(() => setView(null));
  }, []);

  if (!view) return <Card title={t("licence.title")}><Spinner label={t("app.loading")} /></Card>;
  const s = view.status;
  const err = (e: unknown) => setMsg({ tone: "bad", text: t(`licence.error.${e instanceof LicenceError ? e.code : "server"}`, { defaultValue: t("licence.error.server") }) });

  const headline =
    s.kind === "lifetime"
      ? t("licence.lifetime")
      : s.plan === "pro"
        ? t("licence.proUntil", { date: s.until ? day(s.until, lang) : "—" })
        : s.plan === "trial"
          ? t("licence.trialLeft", { count: s.days_left ?? 0 })
          : t("licence.free");

  // Early access: everything is free, nothing to buy or activate.
  if (view.early_access)
    return (
      <Card title={t("licence.title")}>
        <div className="licence-head">
          <span className="dot dot-on" aria-hidden="true" />
          <strong>{t("licence.earlyTitle")}</strong>
        </div>
        <p className="muted small">{t("licence.earlyText")}</p>
      </Card>
    );

  async function run<T>(what: typeof busy, f: () => Promise<T>, ok?: string) {
    setBusy(what);
    setMsg(null);
    try {
      await f();
      if (ok) setMsg({ tone: "ok", text: ok });
      onChanged();
    } catch (e) {
      err(e);
    } finally {
      setBusy(null);
    }
  }

  return (
    <Card title={t("licence.title")}>
      <div className="licence-head">
        <span className={`dot ${s.plan === "community" ? "" : "dot-on"}`} aria-hidden="true" />
        <strong>{headline}</strong>
      </div>
      {s.plan === "community" && <p className="muted small">{s.trial_refused ? t("licence.trialUsed") : t("licence.freeHint")}</p>}
      {s.plan === "trial" && <p className="muted small">{t("licence.trialHint")}</p>}

      {view.key_masked ? (
        <div className="row between wrap">
          <span className="small">
            {t("licence.key")} <code>{view.key_masked}</code>
          </span>
          {confirmRemove ? (
            <span className="row">
              <span className="warn-text small">{t("licence.removeConfirm")}</span>
              <button className="btn-ghost btn-small" onClick={() => setConfirmRemove(false)}>
                {t("health.cancel")}
              </button>
              <button className="btn btn-small" disabled={busy !== null} onClick={() => run("remove", async () => setView(await deactivate(view)), t("licence.removed"))}>
                {t("health.confirm")}
              </button>
            </span>
          ) : (
            <button className="btn-ghost btn-small" onClick={() => setConfirmRemove(true)}>
              {t("licence.remove")}
            </button>
          )}
        </div>
      ) : (
        <>
          <form
            className="row wrap"
            onSubmit={(e) => {
              e.preventDefault();
              if (key.trim()) void run("activate", async () => setView(await activateKey(view, key)), t("licence.activated"));
            }}
          >
            <input className="grow mono" value={key} placeholder="PRO-XXXX-XXXX-XXXX-XXXX" aria-label={t("licence.pasteKey")} onChange={(e) => setKey(e.target.value)} spellCheck={false} autoComplete="off" />
            <button className="btn" type="submit" disabled={busy !== null || !key.trim()}>
              {busy === "activate" ? t("licence.activating") : t("licence.activate")}
            </button>
          </form>
          <div className="row wrap">
            <button className="btn-ghost" onClick={() => openUrl(pricingUrl(view, lang))}>
              {t("licence.buy")}
            </button>
            <button className="btn-ghost btn-small" onClick={() => setShowRecover((v) => !v)}>
              {t("licence.lost")}
            </button>
          </div>
          {showRecover && (
            <form
              className="row wrap"
              onSubmit={(e) => {
                e.preventDefault();
                if (email.trim()) void run("recover", () => recoverKeys(view, email, lang), t("licence.recoverSent"));
              }}
            >
              <input className="grow" type="email" value={email} placeholder={t("licence.email")} aria-label={t("licence.email")} onChange={(e) => setEmail(e.target.value)} />
              <button className="btn-ghost" type="submit" disabled={busy !== null || !email.trim()}>
                {t("licence.recover")}
              </button>
            </form>
          )}
        </>
      )}
      {msg && <p className={`small ${msg.tone === "ok" ? "ok-text" : "bad-text"}`}>{msg.text}</p>}
      <p className="muted small">{t("licence.privacy")}</p>
    </Card>
  );
}
