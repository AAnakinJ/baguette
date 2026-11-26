import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../lib/api";
import type { AppStatus, ReportItem, Shot, TaskRecord } from "../lib/types";
import { duration, money } from "../lib/format";
import { Card, Spinner } from "../lib/ui";
import { ShareDemo } from "./ShareDemo";

const ICON: Record<string, string> = { passed: "✓", failed: "✗", needs_approval: "?", manual: "•", error: "!" };

export function Report({
  task,
  status,
  busy,
  onTask,
  onRetry,
  onVerify,
  onNew,
  onFollowUp,
  setError,
}: {
  task: TaskRecord;
  status: AppStatus;
  busy: string | null;
  onTask: (t: TaskRecord) => void;
  onRetry: () => void;
  onVerify: () => void;
  onNew: () => void;
  onFollowUp: (request: string) => void;
  setError: (e: unknown) => void;
}) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const s = status.settings;
  const attempt = task.attempts[task.attempts.length - 1];
  const report = task.report;
  const isPro = status.plan !== "community";
  const verdict = report?.verdict;
  const tone = verdict === "success" ? "ok" : verdict === "pending" ? "warn" : verdict ? "bad" : task.status === "stopped" ? "warn" : "bad";

  async function act(f: () => Promise<TaskRecord>) {
    try {
      onTask(await f());
    } catch (e) {
      setError(e);
    }
  }

  return (
    <div className="stack">
      <Card tone={tone} title={task.spec?.title ?? t("task.reportTitle")}>
        {task.parent_id && <p className="small muted">↳ {t("followUp.badge")}</p>}
        {report && (
          <div className={`verdict verdict-${report.verdict}`}>
            <span className="verdict-label">{t(`verdict.${report.verdict}`)}</span>
            <span className="muted">
              {report.passed} {t("item.passed").toLowerCase()} · {report.failed} {t("item.failed").toLowerCase()} · {report.pending} {t("item.manual").toLowerCase()}
            </span>
          </div>
        )}
        {attempt?.stopped === "budget" && <p className="warn-text">{t("task.stoppedBudget")}</p>}
        {attempt?.stopped === "cancelled" && <p className="warn-text">{t("task.stoppedUser")}</p>}
        {attempt?.stopped === "retry_limit" && <p className="warn-text">{t("task.stoppedRetryLimit")}</p>}
        {!report && !attempt?.stopped && <p className="bad-text">{t("task.failedRun")}</p>}
        <div className="row wrap small muted">
          <span>
            {t("task.actualCost", { amount: money(task.total_cost_usd, s, lang) })}
            {task.quote && ` (${t("task.quoted", { amount: money(task.quote.likely_usd, s, lang) })})`}
          </span>
          {attempt && attempt.duration_ms > 0 && <span>{t("task.duration", { value: duration(attempt.duration_ms, lang) })}</span>}
        </div>
        {s.ui_mode === "expert" && task.timings && task.timings.run_ms > 0 && (
          <p className="small muted num">
            {t("task.timings", {
              prepare: duration(task.timings.prepare_ms + task.timings.brief_ms, lang),
              run: duration(task.timings.run_ms, lang),
              verify: duration(task.timings.screens_ms + task.timings.verify_ms, lang),
            })}
            {task.attempts.length > 1 && ` · ${t("task.autoRetried", { count: task.attempts.length - 1 })}`}
          </p>
        )}
      </Card>

      {report && (
        <Card title={t("task.criteriaTitle")}>
          {report.items.every((i) => i.kind === "manual" || i.status === "manual") && !isPro && <p className="muted small">{t("task.manualChecklist")}</p>}
          <ul className="report">
            {report.items.map((it) => (
              <ReportRow
                key={it.id}
                it={it}
                onApprove={() => act(async () => {
                  await api.taskApproveCommand(task.id, it.detail);
                  return api.taskVerify(task.id);
                })}
                onManual={(passed) => act(() => api.taskSetManual(task.id, it.id, passed))}
              />
            ))}
          </ul>
          {busy === "verifying" && <Spinner label={t("task.verifying")} />}
          {report.failed > 0 && (
            <div className="retry">
              {isPro ? (
                <>
                  <button className="btn" disabled={busy !== null} onClick={onRetry}>
                    {t("task.retry")}
                  </button>
                  <span className="muted small">{t("task.retryHint")}</span>
                </>
              ) : (
                <span className="muted small">{t("task.retryPro")}</span>
              )}
            </div>
          )}
          {report.items.some((i) => i.status === "error") && (
            <button className="btn-ghost" disabled={busy !== null} onClick={onVerify}>
              ↻ {t("onboarding.recheck")}
            </button>
          )}
        </Card>
      )}

      {attempt?.screenshots && attempt.screenshots.length > 0 && <Screens shots={attempt.screenshots} />}

      {attempt?.denials && attempt.denials.length > 0 && (
        <Card title={t("task.denialsTitle")} tone="warn">
          <p className="small">{t("task.denialsText")}</p>
          <ul className="plain-list mono small">
            {attempt.denials.map((d, i) => (
              <li key={i}>{d.summary || d.tool_name}</li>
            ))}
          </ul>
        </Card>
      )}

      {attempt?.leaks && attempt.leaks.length > 0 && (
        <Card title={t("task.leaksTitle")} tone="bad">
          <p className="small">{t("task.leaksText")}</p>
          <ul className="report">
            {attempt.leaks.map((l, i) => (
              <li key={i} className="report-item st-failed">
                <span className="icon" aria-hidden="true">
                  ✗
                </span>
                <div className="grow">
                  <div className="row between">
                    <code className="small">
                      {l.path}
                      {l.line ? `:${l.line}` : ""}
                    </code>
                    <span className="tag">{t(`leak.${l.kind}`)}</span>
                  </div>
                  {l.preview && <pre className="evidence">{l.preview}</pre>}
                </div>
              </li>
            ))}
          </ul>
        </Card>
      )}

      {attempt?.changes && attempt.changes.length > 0 && (
        <Card
          title={t("task.changesTitle", { count: attempt.changes.length })}
          actions={attempt.snapshot_before ? <UndoTask projectId={task.project_id} snapshot={attempt.snapshot_before} /> : null}
        >
          <ul className="plain-list small">
            {attempt.changes.map((c, i) => (
              <li key={i} className="row">
                <span className={`tag ${c.change === "deleted" ? "tag-failed" : c.change === "added" ? "tag-done" : ""}`}>{t(`task.change.${c.change}`)}</span>
                <code>{c.path}</code>
              </li>
            ))}
          </ul>
        </Card>
      )}

      {attempt?.summary && (
        <Card title={t("task.summary")}>
          <p className="prewrap">{attempt.summary}</p>
        </Card>
      )}

      {report && <ShareDemo task={task} status={status} />}

      {attempt?.session_id && <FollowUp busy={busy} onFollowUp={onFollowUp} />}

      <div className="row end">
        <button className="btn-ghost" onClick={onNew}>
          {t("task.newTask")}
        </button>
      </div>
    </div>
  );
}

/** "Continue this task": the next request reuses the same Claude Code session,
 *  so Claude already knows what it just did and the brief stays short. */
function FollowUp({ busy, onFollowUp }: { busy: string | null; onFollowUp: (request: string) => void }) {
  const { t } = useTranslation();
  const [text, setText] = useState("");
  const send = () => {
    if (text.trim() && !busy) onFollowUp(text.trim());
  };
  return (
    <Card title={t("followUp.title")}>
      <p className="muted small">{t("followUp.hint")}</p>
      <textarea
        className="compose"
        rows={3}
        value={text}
        placeholder={t("followUp.placeholder")}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) send();
        }}
      />
      <div className="row end">
        {busy === "preparing" || busy === "planning" ? (
          <Spinner label={t("task.preparing")} />
        ) : (
          <button className="btn" disabled={!text.trim() || !!busy} onClick={send}>
            {t("followUp.submit")}
          </button>
        )}
      </div>
    </Card>
  );
}

/** Screenshots of the result, taken by the app during the verification. */
function Screens({ shots }: { shots: Shot[] }) {
  const { t } = useTranslation();
  const [urls, setUrls] = useState<Record<string, string>>({});
  const [zoom, setZoom] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    Promise.all(shots.map(async (s) => [s.path, await api.taskScreenshot(s.path).catch(() => "")] as const)).then((pairs) => {
      if (!cancelled) setUrls(Object.fromEntries(pairs.filter(([, u]) => u)));
    });
    return () => {
      cancelled = true;
    };
  }, [shots]);
  return (
    <Card title={t("task.screensTitle")}>
      <p className="muted small">{t("task.screensHint")}</p>
      <div className="shots">
        {shots.map((s) => (
          <figure key={s.path} className={`shot shot-${s.device}`}>
            {urls[s.path] ? (
              <button className="shot-btn" onClick={() => setZoom(s.path)} aria-label={t("task.screensZoom")}>
                <img src={urls[s.path]} alt={t(`task.device.${s.device}`)} width={s.width} height={s.height} />
              </button>
            ) : (
              <div className="shot-ph" style={{ aspectRatio: `${s.width} / ${s.height}` }} />
            )}
            <figcaption className="small muted">
              {t(`task.device.${s.device}`)} · <span className="num">{s.width}×{s.height}</span>
            </figcaption>
          </figure>
        ))}
      </div>
      {zoom && urls[zoom] && (
        <div className="lightbox" role="dialog" aria-modal="true" onClick={() => setZoom(null)}>
          <img src={urls[zoom]} alt="" />
        </div>
      )}
    </Card>
  );
}

function UndoTask({ projectId, snapshot }: { projectId: string; snapshot: string }) {
  const { t } = useTranslation();
  const [step, setStep] = useState<"idle" | "confirm" | "done" | "error">("idle");
  if (step === "done") return <span className="ok-text small">✓ {t("timeline.restored")}</span>;
  if (step === "confirm")
    return (
      <span className="row">
        <span className="warn-text small">{t("task.undoConfirm")}</span>
        <button className="btn-ghost btn-small" onClick={() => setStep("idle")}>
          {t("health.cancel")}
        </button>
        <button
          className="btn btn-small"
          onClick={async () => {
            try {
              await api.timelineRestore(projectId, snapshot);
              setStep("done");
            } catch {
              setStep("error");
            }
          }}
        >
          {t("health.confirm")}
        </button>
      </span>
    );
  return (
    <span className="row">
      {step === "error" && <span className="bad-text small">{t("errors.unknown")}</span>}
      <button className="btn-ghost btn-small" onClick={() => setStep("confirm")}>
        ↺ {t("task.undoTask")}
      </button>
    </span>
  );
}

function ReportRow({ it, onApprove, onManual }: { it: ReportItem; onApprove: () => void; onManual: (passed: boolean) => void }) {
  const { t } = useTranslation();
  return (
    <li className={`report-item st-${it.status}`}>
      <span className="icon" aria-hidden="true">
        {ICON[it.status]}
      </span>
      <div className="grow">
        <div className="row between">
          <span>{it.description}</span>
          <span className="tag">{t(`item.${it.status}`)}</span>
        </div>
        {it.detail && <code className="small">{it.detail}</code>}
        {it.evidence && (
          <details>
            <summary className="small muted">…</summary>
            <pre className="evidence">{it.evidence}</pre>
          </details>
        )}
        {it.status === "needs_approval" && (
          <button className="btn-ghost btn-small" onClick={onApprove}>
            {t("task.approve")}
          </button>
        )}
        {it.kind === "manual" && (
          <div className="row small">
            <span className="muted">{t("task.manualQuestion")}</span>
            <button className="btn-ghost btn-small" onClick={() => onManual(true)}>
              {t("task.yes")}
            </button>
            <button className="btn-ghost btn-small" onClick={() => onManual(false)}>
              {t("task.no")}
            </button>
          </div>
        )}
      </div>
    </li>
  );
}
