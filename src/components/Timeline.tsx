import { useCallback, useEffect, useState } from "react";
import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import { api } from "../lib/api";
import type { Snapshot } from "../lib/types";
import { date } from "../lib/format";
import { Card, ErrorBanner } from "../lib/ui";

/** "before:<task>:<title>" -> translated label. */
export function snapshotLabel(label: string, t: TFunction): string {
  const [kind, , ...rest] = label.split(":");
  const title = rest.join(":");
  if (kind === "before") return t("timeline.before", { title });
  if (kind === "after") return t("timeline.after", { title });
  if (kind === "restore") return t("timeline.restorePoint");
  return label;
}

export function Timeline({ projectId }: { projectId: string }) {
  const { t, i18n } = useTranslation();
  const [snaps, setSnaps] = useState<Snapshot[] | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);

  const load = useCallback(() => {
    api.timelineList(projectId).then(setSnaps).catch(setError);
  }, [projectId]);
  useEffect(load, [load]);

  async function restore(id: string) {
    setConfirm(null);
    try {
      await api.timelineRestore(projectId, id);
      setMsg(t("timeline.restored"));
      load();
    } catch (e) {
      setError(e);
    }
  }

  return (
    <Card title={t("timeline.title")}>
      <ErrorBanner error={error} t={t} onClose={() => setError(null)} />
      <p className="muted small">{t("timeline.intro")}</p>
      {snaps && snaps.length === 0 && <p className="muted small">{t("timeline.empty")}</p>}
      <ul className="history">
        {(snaps ?? []).map((s) => (
          <li key={s.id}>
            <div className="link-row">
              <span className="grow">
                <span>{snapshotLabel(s.label, t)}</span>
                <span className="muted small mono">
                  {date(s.at, i18n.language)} · {s.id.slice(0, 7)}
                </span>
              </span>
              {confirm === s.id ? (
                <span className="row">
                  <button className="btn-ghost btn-small" onClick={() => setConfirm(null)}>
                    {t("health.cancel")}
                  </button>
                  <button className="btn btn-small" onClick={() => restore(s.id)}>
                    {t("health.confirm")}
                  </button>
                </span>
              ) : (
                <button className="btn-ghost btn-small" onClick={() => setConfirm(s.id)}>
                  {t("timeline.restore")}
                </button>
              )}
            </div>
            {confirm === s.id && <p className="warn-text small">{t("timeline.confirm")}</p>}
          </li>
        ))}
      </ul>
      {msg && <p className="ok-text small">✓ {msg}</p>}
    </Card>
  );
}
