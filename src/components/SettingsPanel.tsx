import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../lib/api";
import type { AppStatus, Autonomy, Caps, Currency, Settings, SubscriptionPlan } from "../lib/types";
import { applyLanguage, LANGUAGES } from "../i18n";
import { Card, ErrorBanner, Segmented } from "../lib/ui";
import { ConnectionChooser } from "./Onboarding";
import { LicenceCard } from "./LicenceCard";
import { UpdateCard } from "./Updates";

function CapInput({ label, value, onChange, locked, rate, currency }: { label: string; value: number | null; onChange: (v: number | null) => void; locked: boolean; rate: number; currency: Currency }) {
  const { t } = useTranslation();
  // Caps are stored in USD; shown in the user's currency.
  const shown = value === null ? "" : String(Math.round((currency === "eur" ? value * rate : value) * 100) / 100);
  return (
    <label className="field">
      <span>
        {label} {locked && <em className="badge">{t("settings.proOnly")}</em>}
      </span>
      <div className="row">
        <input
          type="number"
          min={0}
          step={0.5}
          disabled={locked}
          value={shown}
          placeholder={t("settings.noCap")}
          onChange={(e) => {
            const v = e.target.value === "" ? null : Number(e.target.value);
            onChange(v === null || Number.isNaN(v) ? null : currency === "eur" ? v / rate : v);
          }}
        />
        <span className="muted">{currency === "eur" ? "€" : "$"}</span>
      </div>
    </label>
  );
}

export function SettingsPanel({ status, save, refresh }: { status: AppStatus; save: (s: Settings) => Promise<void>; refresh: () => Promise<void> }) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState<Settings>(status.settings);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<unknown>(null);
  useEffect(() => setDraft(status.settings), [status.settings]);
  const community = status.plan === "community";

  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => {
    setSaved(false);
    setDraft({ ...draft, [k]: v });
  };
  const setCap = (k: keyof Caps, v: number | null) => set("caps", { ...draft.caps, [k]: v });
  const setIrr = (k: keyof Settings["irreversible"], v: boolean) => set("irreversible", { ...draft.irreversible, [k]: v });

  return (
    <div className="stack narrow">
      <ErrorBanner error={error} t={t} onClose={() => setError(null)} />
      <Card
        title={t("settings.title")}
        actions={
          <button
            className="btn"
            onClick={async () => {
              try {
                await save(draft);
                applyLanguage(draft.language);
                setSaved(true);
              } catch (e) {
                setError(e);
              }
            }}
          >
            {saved ? `✓ ${t("settings.saved")}` : t("settings.save")}
          </button>
        }
      >
        <div className="grid-2">
          <label className="field">
            <span>{t("settings.language")}</span>
            <select value={draft.language} onChange={(e) => set("language", e.target.value)}>
              {LANGUAGES.map((l) => (
                <option key={l.code} value={l.code}>
                  {l.name}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            <span>{t("settings.currency")}</span>
            <select value={draft.currency} onChange={(e) => set("currency", e.target.value as Currency)}>
              <option value="eur">EUR (€)</option>
              <option value="usd">USD ($)</option>
            </select>
          </label>
        </div>
        <div className="field">
          <span>{t("settings.mode")}</span>
          <Segmented
            label={t("settings.mode")}
            value={draft.ui_mode}
            options={[
              { value: "beginner", label: t("mode.beginner") },
              { value: "expert", label: t("mode.expert") },
            ]}
            onChange={(v) => set("ui_mode", v)}
          />
          <small className="muted">{t("settings.modeHelp")}</small>
        </div>
        <div className="field">
          <span>{t("settings.autonomy")}</span>
          <div className="choice-grid three">
            {(["careful", "balanced", "autopilot"] as Autonomy[]).map((a) => (
              <button key={a} className={`choice ${draft.autonomy === a ? "on" : ""}`} onClick={() => set("autonomy", a)}>
                <strong>{t(`settings.${a}`)}</strong>
                <span>{t(`settings.${a}Desc`)}</span>
              </button>
            ))}
          </div>
        </div>
      </Card>

      <LicenceCard onChanged={refresh} />

      <UpdateCard status={status} />

      <Card title={t("settings.caps")}>
        <p className="muted small">{t("settings.capsHelp")}</p>
        <div className="grid-3">
          <CapInput label={t("settings.perTask")} value={draft.caps.per_task_usd} onChange={(v) => setCap("per_task_usd", v)} locked={false} rate={draft.usd_to_eur} currency={draft.currency} />
          <CapInput label={t("settings.perDay")} value={draft.caps.per_day_usd} onChange={(v) => setCap("per_day_usd", v)} locked={community} rate={draft.usd_to_eur} currency={draft.currency} />
          <CapInput label={t("settings.perMonth")} value={draft.caps.per_month_usd} onChange={(v) => setCap("per_month_usd", v)} locked={community} rate={draft.usd_to_eur} currency={draft.currency} />
        </div>
      </Card>

      <Card title={t("settings.proof")}>
        <label className="toggle">
          <input type="checkbox" disabled={community} checked={draft.screenshots && !community} onChange={(e) => set("screenshots", e.target.checked)} />
          <span>
            {t("settings.screenshots")} {community && <em className="badge">{t("settings.proOnly")}</em>}
          </span>
        </label>
        <small className="muted">{t("settings.screenshotsHelp")}</small>
        <label className="toggle">
          <input type="checkbox" disabled={community} checked={draft.auto_retry && !community} onChange={(e) => set("auto_retry", e.target.checked)} />
          <span>{t("settings.autoRetry")}</span>
        </label>
        <small className="muted">{t("settings.autoRetryHelp")}</small>
      </Card>

      <Card title={t("settings.irreversible")}>
        <p className="muted small">{t("settings.irreversibleHelp")}</p>
        {(
          [
            ["allow_git_push", "allowGitPush"],
            ["allow_deploy", "allowDeploy"],
            ["allow_publish", "allowPublish"],
            ["allow_destructive_delete", "allowDelete"],
          ] as const
        ).map(([k, label]) => (
          <label key={k} className="toggle">
            <input type="checkbox" checked={draft.irreversible[k]} onChange={(e) => setIrr(k, e.target.checked)} />
            <span>{t(`settings.${label}`)}</span>
          </label>
        ))}
      </Card>

      <Card title={t("settings.connection")}>
        <ConnectionChooser status={status} save={save} refresh={refresh} />
        {status.settings.connection_mode === "subscription" && (
          <label className="field">
            <span>{t("settings.plan")}</span>
            <select value={draft.plan} onChange={(e) => set("plan", e.target.value as SubscriptionPlan)}>
              {["pro", "max5x", "max20x", "ultra", "team"].map((p) => (
                <option key={p} value={p}>
                  {p.toUpperCase()}
                </option>
              ))}
            </select>
          </label>
        )}
        {status.settings.connection_mode === "api_key" && status.api_key_set && (
          <button
            className="btn-ghost"
            onClick={async () => {
              await api.clearApiKey();
              await refresh();
            }}
          >
            {t("settings.removeKey")}
          </button>
        )}
      </Card>
    </div>
  );
}
