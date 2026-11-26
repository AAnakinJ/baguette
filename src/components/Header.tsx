import { useTranslation } from "react-i18next";
import type { AppStatus, Settings } from "../lib/types";
import { money } from "../lib/format";
import { Segmented } from "../lib/ui";
import { Logo } from "./Logo";

export function Header({ status, save, onSettings }: { status: AppStatus; save: (s: Settings) => Promise<void>; onSettings: () => void }) {
  const { t, i18n } = useTranslation();
  const s = status.settings;
  const sub = s.connection_mode === "subscription";
  const ready = status.claude.found && status.claude.version_ok && (sub ? status.claude.subscription_signed_in : status.api_key_set);
  const problem = !status.claude.found ? t("status.missing") : sub && !status.claude.subscription_signed_in ? t("status.signedOut") : !sub && !status.api_key_set ? t("status.noKey") : null;
  const days = status.licence?.days_left;
  const planLabel = status.early_access ? t("app.earlyAccess") : status.plan === "pro" ? t("app.pro") : status.plan === "trial" ? (days != null ? t("app.trialDays", { count: days }) : t("app.trial")) : t("app.community");

  return (
    <header className="topbar">
      <div className="brand">
        <Logo size={22} />
        <div>
          <strong>{status.product}</strong>
          <span className="tagline">{t("app.tagline")}</span>
        </div>
      </div>
      <Segmented
        label={t("settings.mode")}
        value={s.ui_mode}
        options={[
          { value: "beginner", label: t("mode.beginner") },
          { value: "expert", label: t("mode.expert") },
        ]}
        onChange={(v) => save({ ...s, ui_mode: v })}
      />
      <div className="topbar-right">
        <div className="spend" title={sub ? "API" : undefined}>
          <span>
            {t("status.today")} <b>{money(status.spent.today_usd, s, i18n.language)}</b>
          </span>
          <span>
            {t("status.month")} <b>{money(status.spent.month_usd, s, i18n.language)}</b>
          </span>
        </div>
        <button className={`chip ${ready ? "chip-ok" : "chip-bad"}`} onClick={onSettings}>
          <span className="dot" aria-hidden="true" />
          {ready ? `${t("status.ready")}${sub && status.claude.plan ? ` · ${status.claude.plan.toUpperCase()}` : ""}` : problem}
        </button>
        <span className="badge">{planLabel}</span>
      </div>
    </header>
  );
}
