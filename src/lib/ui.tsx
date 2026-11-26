import type { TFunction } from "i18next";
import type { ReactNode } from "react";
import { isApiError } from "./api";

export function errorText(e: unknown, t: TFunction): string {
  if (isApiError(e)) return t(`errors.${e.code}`, { defaultValue: t("errors.unknown") });
  return t("errors.unknown");
}

export function ErrorBanner({ error, t, onClose }: { error: unknown; t: TFunction; onClose?: () => void }) {
  if (!error) return null;
  return (
    <div className="banner banner-bad" role="alert">
      <span>{errorText(error, t)}</span>
      {onClose && (
        <button className="btn-ghost btn-small" onClick={onClose} aria-label="close">
          ×
        </button>
      )}
    </div>
  );
}

export function Spinner({ label }: { label: string }) {
  return (
    <div className="spinner-row" role="status" aria-live="polite">
      <span className="spinner" aria-hidden="true" />
      <span>{label}</span>
    </div>
  );
}

export function Card({ title, children, actions, tone }: { title?: ReactNode; children: ReactNode; actions?: ReactNode; tone?: "ok" | "warn" | "bad" }) {
  return (
    <section className={`card${tone ? ` card-${tone}` : ""}`}>
      {(title || actions) && (
        <header className="card-head">
          {title && <h2>{title}</h2>}
          {actions && <div className="card-actions">{actions}</div>}
        </header>
      )}
      {children}
    </section>
  );
}

export function Segmented<T extends string>({ value, options, onChange, label }: { value: T; options: { value: T; label: string }[]; onChange: (v: T) => void; label: string }) {
  return (
    <div className="segmented" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button key={o.value} role="radio" aria-checked={value === o.value} className={value === o.value ? "on" : ""} onClick={() => onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}
