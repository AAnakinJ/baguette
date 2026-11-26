import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api, openUrl } from "../lib/api";
import { useEvent } from "../lib/useEvent";
import type { AppStatus, ConnectionMode, Settings } from "../lib/types";
import { applyLanguage, LANGUAGES } from "../i18n";
import { ErrorBanner, Spinner } from "../lib/ui";
import { Logo } from "./Logo";

const INSTALL_URL = "https://code.claude.com/docs/en/setup";
const CONSOLE_URL = "https://platform.claude.com/settings/keys";

export function ConnectionChooser({ status, save, refresh }: { status: AppStatus; save: (s: Settings) => Promise<void>; refresh: () => Promise<void> }) {
  const { t } = useTranslation();
  const s = status.settings;
  const [key, setKey] = useState("");
  const [msg, setMsg] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [waiting, setWaiting] = useState(false);
  const [loginUrl, setLoginUrl] = useState<string | null>(null);

  useEvent<string>("login-url", setLoginUrl);
  useEvent<boolean>("login-finished", () => {
    setWaiting(false);
    void refresh();
  });

  const choose = (mode: ConnectionMode) => save({ ...s, connection_mode: mode });

  return (
    <div className="stack">
      <ErrorBanner error={error} t={t} onClose={() => setError(null)} />
      <div className="choice-grid">
        <button className={`choice ${s.connection_mode === "api_key" ? "on" : ""}`} onClick={() => choose("api_key")}>
          <strong>{t("onboarding.apiKey")}</strong>
          <span>{t("onboarding.apiKeyDesc")}</span>
        </button>
        <button
          className={`choice ${s.connection_mode === "subscription" ? "on" : ""}`}
          disabled={!status.subscription_mode_enabled}
          onClick={() => choose("subscription")}
        >
          <strong>{t("onboarding.subscription")}</strong>
          <span>{status.subscription_mode_enabled ? t("onboarding.subscriptionDesc") : t("onboarding.subscriptionOff")}</span>
        </button>
      </div>

      {s.connection_mode === "api_key" ? (
        <div className="stack">
          <label className="field">
            <span>{t("onboarding.keyLabel")}</span>
            <input type="password" autoComplete="off" spellCheck={false} value={key} onChange={(e) => setKey(e.target.value)} placeholder="sk-ant-…" />
            <small>{t("onboarding.keyHelp")}</small>
          </label>
          <div className="row">
            <button
              className="btn"
              disabled={!key.trim()}
              onClick={async () => {
                setMsg(null);
                if (!key.trim().startsWith("sk-ant-")) {
                  setMsg(t("onboarding.keyInvalid"));
                  return;
                }
                try {
                  await api.setApiKey(key);
                  setKey("");
                  setMsg(t("onboarding.keySaved"));
                  await refresh();
                } catch (e) {
                  setError(e);
                }
              }}
            >
              {t("onboarding.saveKey")}
            </button>
            <button className="btn-ghost" onClick={() => openUrl(CONSOLE_URL)}>
              {t("onboarding.getKey")} ↗
            </button>
          </div>
          {msg && <p className="muted">{msg}</p>}
          {status.api_key_set && !msg && <p className="ok-text">✓ {t("onboarding.keySaved")}</p>}
        </div>
      ) : status.claude.subscription_signed_in ? (
        <p className="ok-text">✓ {t("onboarding.signedIn", { plan: (status.claude.plan ?? "").toUpperCase() })}</p>
      ) : (
        <div className="stack">
          {waiting ? <Spinner label={t("onboarding.signInWaiting")} /> : null}
          <div className="row">
            <button
              className="btn"
              disabled={waiting || !status.claude.found}
              onClick={async () => {
                try {
                  setWaiting(true);
                  await api.startLogin();
                } catch (e) {
                  setWaiting(false);
                  setError(e);
                }
              }}
            >
              {t("onboarding.signIn")}
            </button>
            {loginUrl && (
              <button className="btn-ghost" onClick={() => openUrl(loginUrl)}>
                {t("onboarding.signInLink")} ↗
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

/** Installs Claude Code with Anthropic's official installer, no terminal. */
function AutoInstall({ refresh }: { refresh: () => Promise<void> }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const [log, setLog] = useState<string[]>([]);
  const [failed, setFailed] = useState(false);
  useEvent<string>("setup-log", (l) => setLog((x) => [...x.slice(-100), l]));
  return (
    <div className="stack">
      <p>{t("onboarding.claudeMissingAuto")}</p>
      <div className="row">
        <button
          className="btn"
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            setFailed(false);
            setLog([]);
            try {
              const ok = await api.setupRun("claude_code", false);
              setFailed(!ok);
              await refresh();
            } catch {
              setFailed(true);
            } finally {
              setBusy(false);
            }
          }}
        >
          {t("setup.autoInstall")}
        </button>
        <button className="btn-ghost" onClick={() => openUrl(INSTALL_URL)}>
          {t("onboarding.claudeInstall")} ↗
        </button>
      </div>
      {busy && <Spinner label={t("health.installing")} />}
      {failed && <p className="bad-text small">{t("health.failed")}</p>}
      {log.length > 0 && (
        <ol className="feed">
          {log.map((l, i) => (
            <li key={i} className="feed-tool">
              {l}
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}

export function Onboarding({ status, refresh, save }: { status: AppStatus; refresh: () => Promise<void>; save: (s: Settings) => Promise<void> }) {
  const { t } = useTranslation();
  const [step, setStep] = useState(0);
  const s = status.settings;
  const c = status.claude;
  const connected = s.connection_mode === "subscription" ? c.subscription_signed_in : status.api_key_set;

  return (
    <div className="center-screen">
      <div className="onboarding card">
        <div className="brand brand-big">
          <Logo size={32} />
          <div>
            <strong>{status.product}</strong>
            <span className="tagline">{t("app.tagline")}</span>
          </div>
        </div>
        <ol className="steps" aria-label="steps">
          {[0, 1, 2].map((i) => (
            <li key={i} className={i === step ? "on" : i < step ? "done" : ""} />
          ))}
        </ol>

        {step === 0 && (
          <div className="stack">
            <h1>{t("onboarding.welcomeTitle")}</h1>
            <p className="lead">{t("onboarding.welcomeText")}</p>
            <label className="field">
              <span>{t("onboarding.language")}</span>
              <select
                value={s.language}
                onChange={async (e) => {
                  applyLanguage(e.target.value);
                  await save({ ...s, language: e.target.value });
                }}
              >
                {LANGUAGES.map((l) => (
                  <option key={l.code} value={l.code}>
                    {l.name}
                  </option>
                ))}
              </select>
            </label>
            <div className="row end">
              <button className="btn" onClick={() => setStep(1)}>
                {t("onboarding.next")}
              </button>
            </div>
          </div>
        )}

        {step === 1 && (
          <div className="stack">
            <h1>{t("onboarding.claudeTitle")}</h1>
            {c.found && c.version_ok ? (
              <p className="ok-text">✓ {t("onboarding.claudeFound", { version: c.version })}</p>
            ) : c.found ? (
              <p className="warn-text">{t("onboarding.claudeOld", { version: c.version })}</p>
            ) : (
              <AutoInstall refresh={refresh} />
            )}
            <div className="row between">
              <button className="btn-ghost" onClick={() => setStep(0)}>
                {t("onboarding.back")}
              </button>
              <div className="row">
                {!(c.found && c.version_ok) && (
                  <button className="btn-ghost" onClick={refresh}>
                    {t("onboarding.recheck")}
                  </button>
                )}
                <button className="btn" disabled={!c.found} onClick={() => setStep(2)}>
                  {t("onboarding.next")}
                </button>
              </div>
            </div>
          </div>
        )}

        {step === 2 && (
          <div className="stack">
            <h1>{t("onboarding.connectTitle")}</h1>
            <ConnectionChooser status={status} save={save} refresh={refresh} />
            <div className="row between">
              <button className="btn-ghost" onClick={() => setStep(1)}>
                {t("onboarding.back")}
              </button>
              <button className="btn" disabled={!connected} onClick={() => save({ ...s, onboarded: true })}>
                {t("onboarding.finish")}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
