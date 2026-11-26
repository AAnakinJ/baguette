import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../lib/api";
import type { AppStatus, NightEvent, NightItem, NightReport, NightView, TaskRecord } from "../lib/types";
import { clock, date, duration, money, nextAt } from "../lib/format";
import { Moon } from "./Logo";
import { useEvent } from "../lib/useEvent";
import { Card, ErrorBanner, Spinner } from "../lib/ui";

type Live = { current: string | null; pausedUntil: number | null; done: NightItem[] };

/** Night pilot: tonight's queue, options, live progress and the morning report. */
export function NightPanel({ status, onOpenTask }: { status: AppStatus; onOpenTask: (taskId: string, projectId: string) => void }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const s = status.settings;
  const community = status.plan === "community";
  const [view, setView] = useState<NightView | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [budget, setBudget] = useState("");
  const [stopAt, setStopAt] = useState("07:00");
  const [retry, setRetry] = useState(true);
  const [live, setLive] = useState<Live | null>(null);
  const rate = s.currency === "eur" ? s.usd_to_eur : 1;

  const load = useCallback(async () => {
    try {
      setView(await api.nightGet());
    } catch (e) {
      setError(e);
    }
  }, []);
  useEffect(() => {
    void load();
  }, [load]);

  useEvent<NightEvent>("night-event", (ev) => {
    setLive((l) => {
      const cur: Live = l ?? { current: null, pausedUntil: null, done: [] };
      switch (ev.kind) {
        case "started":
          return { current: null, pausedUntil: null, done: [] };
        case "task_started":
          return { ...cur, current: ev.title, pausedUntil: null };
        case "paused":
          return { ...cur, pausedUntil: ev.until };
        case "resumed":
          return { ...cur, pausedUntil: null };
        case "task_finished":
          return { ...cur, current: null, done: [...cur.done, ev.item] };
        case "finished":
          return null;
      }
    });
    if (ev.kind === "finished" || ev.kind === "task_finished") void load();
  });

  async function act(f: () => Promise<NightView>) {
    try {
      setView(await f());
    } catch (e) {
      setError(e);
    }
  }

  async function start() {
    const budgetUsd = budget.trim() === "" ? null : Number(budget) / rate;
    if (budgetUsd !== null && (Number.isNaN(budgetUsd) || budgetUsd <= 0)) return;
    setLive({ current: null, pausedUntil: null, done: [] });
    setView((v) => (v ? { ...v, running: true } : v));
    try {
      await api.nightStart({ budget_usd: budgetUsd, stop_at: nextAt(stopAt), retry_failed: retry, resume_margin_secs: 90 });
    } catch (e) {
      setError(e);
    }
    setLive(null);
    await load();
  }

  if (!view) return <Spinner label={t("app.loading")} />;
  const queue = view.queue;
  const likely = queue.reduce((sum, task) => sum + Math.max(0, (task.quote?.likely_usd ?? 0) - task.total_cost_usd), 0);
  const running = view.running || live !== null;

  return (
    <div className="stack narrow">
      <ErrorBanner error={error} t={t} onClose={() => setError(null)} />
      <Card title={t("night.title")}>
        <p className="muted">{t("night.intro")}</p>
        {community && <p className="warn-text small">{t("night.proOnly")}</p>}
      </Card>

      <Card title={t("night.queueTitle", { count: queue.length })}>
        {queue.length === 0 ? (
          <p className="muted small">{t("night.empty")}</p>
        ) : (
          <ol className="night-queue">
            {queue.map((task, i) => (
              <QueueRow
                key={task.id}
                task={task}
                first={i === 0}
                last={i === queue.length - 1}
                disabled={running}
                amount={money(Math.max(0, (task.quote?.likely_usd ?? 0) - task.total_cost_usd), s, lang)}
                onMove={(d) => act(() => api.nightMove(task.id, d))}
                onRemove={() => act(() => api.nightDequeue(task.id))}
              />
            ))}
          </ol>
        )}
        {queue.length > 0 && <p className="small muted num">{t("night.totalLikely", { amount: money(likely, s, lang) })}</p>}
      </Card>

      {queue.length > 0 && !running && (
        <Card title={t("night.optionsTitle")}>
          <div className="grid-2">
            <label className="field">
              <span>{t("night.budget")}</span>
              <div className="row">
                <input type="number" min={0} step={0.5} value={budget} placeholder={t("settings.noCap")} onChange={(e) => setBudget(e.target.value)} />
                <span className="muted">{s.currency === "eur" ? "€" : "$"}</span>
              </div>
              <small className="muted">{t("night.budgetHelp")}</small>
            </label>
            <label className="field">
              <span>{t("night.stopAt")}</span>
              <input type="time" value={stopAt} onChange={(e) => setStopAt(e.target.value)} />
              <small className="muted">{t("night.stopAtHelp")}</small>
            </label>
          </div>
          <label className="toggle">
            <input type="checkbox" checked={retry} onChange={(e) => setRetry(e.target.checked)} />
            <span>{t("night.retry")}</span>
          </label>
          <p className="small muted">{t("night.keepOpen")}</p>
          <div className="row end">
            <button className="btn btn-big" disabled={community} onClick={start}>
              <Moon /> {t("night.start", { count: queue.length })}
            </button>
          </div>
        </Card>
      )}

      {running && (
        <Card
          title={t("night.runningTitle")}
          actions={
            <button className="btn-danger" onClick={() => api.nightStop()}>
              {t("night.stop")}
            </button>
          }
        >
          {live?.pausedUntil ? (
            <p className="warn-text">{t("night.paused", { time: clock(live.pausedUntil, lang) })}</p>
          ) : (
            <Spinner label={live?.current ? t("night.working", { title: live.current }) : t("night.preparing")} />
          )}
          {live && live.done.length > 0 && <ItemList items={live.done} lang={lang} status={status} />}
        </Card>
      )}

      {view.last && !running && <MorningReport report={view.last} status={status} onOpen={(id) => {
        const task = queue.find((q) => q.id === id);
        void api.taskGet(id).then((r) => onOpenTask(id, (task ?? r).project_id)).catch(setError);
      }} />}
    </div>
  );
}

function QueueRow({
  task,
  first,
  last,
  disabled,
  amount,
  onMove,
  onRemove,
}: {
  task: TaskRecord;
  first: boolean;
  last: boolean;
  disabled: boolean;
  amount: string;
  onMove: (delta: number) => void;
  onRemove: () => void;
}) {
  const { t } = useTranslation();
  return (
    <li className="night-row">
      <div className="grow">
        <div>{task.spec?.title ?? task.request}</div>
        <div className="small muted num">
          {t("night.likely", { amount })}
          {task.status === "blocked" && <span className="tag tag-failed">{t("night.note.blocked")}</span>}
        </div>
      </div>
      <div className="row">
        <button className="btn-ghost btn-small" disabled={disabled || first} onClick={() => onMove(-1)} aria-label={t("night.up")}>
          ↑
        </button>
        <button className="btn-ghost btn-small" disabled={disabled || last} onClick={() => onMove(1)} aria-label={t("night.down")}>
          ↓
        </button>
        <button className="btn-ghost btn-small" disabled={disabled} onClick={onRemove} aria-label={t("night.remove")}>
          ✕
        </button>
      </div>
    </li>
  );
}

const TAG: Record<string, string> = { done: "tag-done", failed: "tag-failed", stopped: "tag-stopped", skipped: "", not_run: "" };

function ItemList({ items, lang, status, onOpen }: { items: NightItem[]; lang: string; status: AppStatus; onOpen?: (taskId: string) => void }) {
  const { t } = useTranslation();
  return (
    <ul className="plain-list night-items">
      {items.map((it) => (
        <li key={it.task_id} className="night-row">
          <span className={`tag ${TAG[it.outcome]}`}>{t(`night.outcome.${it.outcome}`)}</span>
          <div className="grow">
            {onOpen && it.outcome !== "skipped" && it.outcome !== "not_run" ? (
              <button className="link" onClick={() => onOpen(it.task_id)}>
                {it.title}
              </button>
            ) : (
              <span>{it.title}</span>
            )}
            <div className="small muted num">
              {[
                it.verdict && t(`verdict.${it.verdict}`),
                it.cost_usd > 0 && money(it.cost_usd, status.settings, lang),
                it.duration_ms > 0 && duration(it.duration_ms, lang),
                it.pauses > 0 && t("night.pausedCount", { count: it.pauses }),
                it.retried && t("night.retried"),
                it.note && t(`night.note.${it.note}`, { defaultValue: it.note }),
              ]
                .filter(Boolean)
                .join(" · ")}
            </div>
          </div>
        </li>
      ))}
    </ul>
  );
}

function MorningReport({ report, status, onOpen }: { report: NightReport; status: AppStatus; onOpen: (taskId: string) => void }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const n = (o: string) => report.items.filter((i) => i.outcome === o).length;
  const tone = n("failed") + n("stopped") > 0 ? "warn" : n("done") > 0 ? "ok" : undefined;
  return (
    <Card title={t("night.reportTitle")} tone={tone}>
      <p className="small muted">
        {t("night.reportWhen", { start: date(report.started_at, lang), end: report.finished_at ? clock(report.finished_at, lang) : "…" })}
      </p>
      <div className="night-stats">
        <Stat label={t("night.outcome.done")} value={n("done")} />
        <Stat label={t("night.outcome.failed")} value={n("failed") + n("stopped")} />
        <Stat label={t("night.outcome.skipped")} value={n("skipped") + n("not_run")} />
        <Stat label={t("night.spent")} value={money(report.total_cost_usd, status.settings, lang)} />
      </div>
      {report.pauses.length > 0 && (
        <p className="small muted">
          {t("night.pausesLine", { count: report.pauses.length, time: clock(report.pauses[report.pauses.length - 1].until, lang) })}
        </p>
      )}
      {report.cancelled && <p className="warn-text small">{t("night.cancelled")}</p>}
      <ItemList items={report.items} lang={lang} status={status} onOpen={onOpen} />
    </Card>
  );
}

function Stat({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="stat">
      <span className="stat-value num">{value}</span>
      <span className="small muted">{label}</span>
    </div>
  );
}
