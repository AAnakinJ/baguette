import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, on } from "../lib/api";
import type { Answer, AppStatus, ClientQuote, Effort, LiveEvent, ProjectView, TaskRecord, TaskSpec } from "../lib/types";
import { amount, duration, minutes, money, percent } from "../lib/format";
import { Card, ErrorBanner, Segmented, Spinner } from "../lib/ui";
import { Moon } from "./Logo";
import { Report } from "./Report";

type Busy = null | "preparing" | "planning" | "running" | "verifying" | "asking";

// "default" / "" = keep Claude Code's own choice (shown as "auto").
const MODELS = ["default", "haiku", "sonnet", "opus", "fable"];
const EFFORTS: (Effort | "")[] = ["", "low", "medium", "high", "xhigh", "max"];

export function TaskFlow({
  project,
  status,
  taskId,
  onChanged,
  onNew,
}: {
  project: ProjectView;
  status: AppStatus;
  taskId: string | null;
  onChanged: () => void;
  onNew: () => void;
}) {
  const { t } = useTranslation();
  const expert = status.settings.ui_mode === "expert";
  const [task, setTask] = useState<TaskRecord | null>(null);
  const [request, setRequest] = useState("");
  const [busy, setBusy] = useState<Busy>(null);
  const [error, setError] = useState<unknown>(null);
  const [choice, setChoice] = useState<Record<string, number>>({});
  const [other, setOther] = useState<Record<string, string>>({});
  const [events, setEvents] = useState<LiveEvent[]>([]);
  const [liveCost, setLiveCost] = useState(0);
  const [promptDraft, setPromptDraft] = useState("");
  // "task": change the project, with quote and proof. "question": read-only answer.
  const [mode, setMode] = useState<"task" | "question">("task");

  useEffect(() => {
    if (taskId) api.taskGet(taskId).then(setTask).catch(setError);
  }, [taskId]);

  useEffect(() => setPromptDraft(task?.prompt ?? ""), [task?.prompt]);

  async function guard<T>(kind: Busy, f: () => Promise<T>): Promise<T | undefined> {
    setBusy(kind);
    setError(null);
    try {
      return await f();
    } catch (e) {
      setError(e);
      return undefined;
    } finally {
      setBusy(null);
    }
  }

  async function prepare(start: () => Promise<TaskRecord> = () => api.taskStart(project.project.id, request)) {
    const started = await guard("preparing", start);
    if (!started) return;
    setTask(started);
    setChoice(Object.fromEntries(started.questions.map((q) => [q.key, 0])));
    // Clear requests come back already quoted (one combined call); only drafts need planning.
    if (started.questions.length === 0 && started.status === "draft") await plan(started, []);
    else onChanged();
  }

  async function plan(tk: TaskRecord, answers: Answer[]) {
    const planned = await guard("planning", () => api.taskPlan(tk.id, answers));
    if (planned) setTask(planned);
    onChanged();
  }

  function collectAnswers(tk: TaskRecord): Answer[] {
    return tk.questions.map((q) => {
      const c = choice[q.key] ?? 0;
      const answer = c === -1 ? (other[q.key] ?? "").trim() : q.options[c]?.label ?? q.options[0].label;
      return { key: q.key, question: q.question, answer: answer || q.options[0].label };
    });
  }

  async function run(retry = false) {
    if (!task) return;
    setEvents([]);
    setLiveCost(0);
    const off = await on<LiveEvent>("task-event", (ev) => {
      if (ev.task_id !== task.id) return;
      setEvents((list) => [...list.slice(-300), ev]);
      setLiveCost(ev.cost_so_far_usd);
    });
    const done = await guard("running", () => (retry ? api.taskRetry(task.id) : api.taskRun(task.id)));
    off();
    if (done) setTask(done);
    else api.taskGet(task.id).then(setTask).catch(() => undefined);
    onChanged();
  }

  /** Question mode: read-only, streamed like a run, answered in one go. */
  async function ask() {
    setEvents([]);
    setLiveCost(0);
    const off = await on<LiveEvent>("task-event", (ev) => {
      setEvents((list) => [...list.slice(-300), ev]);
      setLiveCost(ev.cost_so_far_usd);
    });
    const answered = await guard("asking", () => api.taskAsk(project.project.id, request));
    off();
    if (answered) setTask(answered);
    onChanged();
  }

  /** "Continue this task": a new request in the same Claude Code session. */
  async function followUp(text: string) {
    if (!task) return;
    const parent = task.id;
    setEvents([]);
    await prepare(() => api.taskFollowUp(parent, text));
  }

  function askAnother() {
    setTask(null);
    setRequest("");
    setMode("question");
  }

  const stage = busy === "asking" ? "asking" : !task ? "compose" : task.kind === "question" ? "answer" : busy === "running" ? "running" : task.status === "draft" ? "grill" : task.status === "planned" || task.status === "blocked" ? "quote" : "report";
  const question = mode === "question";
  const submit = () => (question ? ask() : prepare());

  return (
    <div className="flow">
      <ErrorBanner error={error} t={t} onClose={() => setError(null)} />

      {stage === "compose" && (
        <Card
          title={question ? t("ask.title") : t("task.composeTitle")}
          actions={
            <Segmented
              label={t("ask.modeLabel")}
              value={mode}
              options={[
                { value: "task", label: t("ask.modeTask") },
                { value: "question", label: t("ask.modeQuestion") },
              ]}
              onChange={setMode}
            />
          }
        >
          <textarea
            className="compose"
            rows={question ? 3 : 5}
            value={request}
            placeholder={question ? t("ask.placeholder") : t("task.composePlaceholder")}
            onChange={(e) => setRequest(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && request.trim()) void submit();
            }}
            autoFocus
          />
          <p className="muted small">{question ? t("ask.hint") : t("task.composeHint")}</p>
          {!expert && !question && (
            <div className="chips" aria-label={t("task.examples")}>
              {["example1", "example2", "example3"].map((k) => (
                <button key={k} className="chip-btn" onClick={() => setRequest(t(`task.${k}`))}>
                  {t(`task.${k}`)}
                </button>
              ))}
            </div>
          )}
          <div className="row end">
            {busy === "preparing" ? (
              <Spinner label={t("task.preparing")} />
            ) : (
              <button className="btn" disabled={!request.trim()} onClick={() => void submit()}>
                {question ? t("ask.submit") : t("task.prepare")}
              </button>
            )}
          </div>
        </Card>
      )}

      {stage === "asking" && (
        <Card
          title={t("ask.searching")}
          actions={
            <button className="btn-danger" onClick={() => api.taskCancel(`ask:${project.project.id}`)}>
              {t("task.stop")}
            </button>
          }
        >
          <p className="muted small prewrap">{request}</p>
          <EventFeed events={events} />
          <Spinner label={t("ask.reading")} />
        </Card>
      )}

      {stage === "answer" && task && <AnswerCard task={task} status={status} onAnother={askAnother} onNew={onNew} />}

      {stage === "grill" && task && (
        <Card title={t("task.grillTitle")}>
          <p className="muted">{t("task.grillIntro")}</p>
          <div className="questions">
            {task.questions.map((q) => (
              <fieldset key={q.key} className="question">
                <legend>{q.question}</legend>
                {q.why && <p className="muted small">{q.why}</p>}
                {q.options.map((o, i) => (
                  <label key={i} className={`option ${choice[q.key] === i ? "on" : ""}`}>
                    <input type="radio" name={q.key} checked={choice[q.key] === i} onChange={() => setChoice({ ...choice, [q.key]: i })} />
                    <span>
                      <strong>{o.label}</strong>
                      {i === 0 && <em className="reco">{t("task.recommended")}</em>}
                      <small>{o.description}</small>
                    </span>
                  </label>
                ))}
                <label className={`option ${choice[q.key] === -1 ? "on" : ""}`}>
                  <input type="radio" name={q.key} checked={choice[q.key] === -1} onChange={() => setChoice({ ...choice, [q.key]: -1 })} />
                  <span className="grow">
                    <strong>{t("task.other")}</strong>
                    {choice[q.key] === -1 && (
                      <input className="other" value={other[q.key] ?? ""} placeholder={t("task.otherPlaceholder")} onChange={(e) => setOther({ ...other, [q.key]: e.target.value })} autoFocus />
                    )}
                  </span>
                </label>
              </fieldset>
            ))}
          </div>
          <div className="row end">
            {busy === "planning" ? (
              <Spinner label={t("task.planning")} />
            ) : (
              <button className="btn" onClick={() => plan(task, collectAnswers(task))}>
                {t("task.continue")}
              </button>
            )}
          </div>
        </Card>
      )}

      {stage === "quote" && task && task.quote && task.spec && (
        <QuoteStage
          task={task}
          status={status}
          busy={busy}
          expert={expert}
          promptDraft={promptDraft}
          setPromptDraft={setPromptDraft}
          onRun={() => run(false)}
          onFullPrep={() => prepare(() => api.taskPrepareFull(task.id))}
          onEdit={() => {
            setRequest(task.request);
            setTask(null);
          }}
          onOverride={async (model, effort, maxTurns) => {
            const r = await guard(null, () => api.taskOverride(task.id, { model, effort, max_turns: maxTurns }));
            if (r) setTask(r);
          }}
          onSavePrompt={async () => {
            const r = await guard(null, () => api.taskEditPrompt(task.id, promptDraft));
            if (r) setTask(r);
          }}
        />
      )}

      {stage === "running" && task && (
        <Card
          title={t("task.liveTitle")}
          actions={
            <button className="btn-danger" onClick={() => api.taskCancel(task.id)}>
              {t("task.stop")}
            </button>
          }
        >
          <CostBar task={task} status={status} spent={liveCost} />
          <EventFeed events={events} />
          <Spinner label={t("task.running")} />
        </Card>
      )}

      {stage === "report" && task && (
        <Report
          task={task}
          status={status}
          busy={busy}
          onTask={setTask}
          onRetry={() => run(true)}
          onVerify={async () => {
            const r = await guard("verifying", () => api.taskVerify(task.id));
            if (r) setTask(r);
          }}
          onNew={onNew}
          onFollowUp={followUp}
          setError={setError}
        />
      )}

      {stage === "compose" && <MemoryCard project={project} />}
    </div>
  );
}

/** Express path: no questions, no brief call, checked by the project's tests only. */
function ExpressNote({ spec, busy, onFullPrep }: { spec: TaskSpec; busy: Busy; onFullPrep: () => void }) {
  const { t } = useTranslation();
  const check = spec.success_criteria[0]?.check;
  const command = check && check.kind === "command" ? check.command : "";
  // The command is shown as code inside the translated sentence.
  const [before, after = ""] = t("express.hint", { command: "\u0000" }).split("\u0000");
  return (
    <div className="express-note row between wrap">
      <p className="small">
        <strong>⚡ {t("express.badge")}</strong> · {before}
        <code>{command}</code>
        {after}
      </p>
      {busy === "preparing" ? (
        <Spinner label={t("task.preparing")} />
      ) : (
        <button className="btn-ghost btn-small" disabled={busy !== null} onClick={onFullPrep}>
          {t("express.full")}
        </button>
      )}
    </div>
  );
}

/** The answer of a question (read-only mode). */
function AnswerCard({ task, status, onAnother, onNew }: { task: TaskRecord; status: AppStatus; onAnother: () => void; onNew: () => void }) {
  const { t, i18n } = useTranslation();
  const a = task.attempts[task.attempts.length - 1];
  const ok = task.status === "done" && !!a?.summary;
  return (
    <Card title={t("ask.answerTitle")} tone={ok ? undefined : "warn"}>
      <p className="muted small prewrap">{task.request}</p>
      {ok ? <div className="answer prewrap">{a!.summary}</div> : <p className="warn-text">{task.status === "stopped" ? t("task.stoppedUser") : t("ask.failed")}</p>}
      <div className="row between wrap">
        <span className="muted small num">
          {money(task.total_cost_usd, status.settings, i18n.language)}
          {a && a.duration_ms > 0 && ` · ${duration(a.duration_ms, i18n.language)}`} · {t("ask.readOnly")}
        </span>
        <span className="row">
          <button className="btn-ghost" onClick={onNew}>
            {t("task.newTask")}
          </button>
          <button className="btn" onClick={onAnother}>
            {t("ask.another")}
          </button>
        </span>
      </div>
    </Card>
  );
}

function MemoryCard({ project }: { project: ProjectView }) {
  const { t } = useTranslation();
  const [items, setItems] = useState(project.memory);
  return (
    <Card title={t("project.memory")}>
      {items.length === 0 ? (
        <p className="muted small">{t("project.memoryEmpty")}</p>
      ) : (
        <ul className="memory">
          {items.map((m) => (
            <li key={m.key}>
              <span>
                <span className="muted">{m.question}</span> <b>{m.answer}</b>
              </span>
              <button className="btn-ghost btn-small" onClick={async () => setItems(await api.forgetMemory(project.project.id, m.key))}>
                {t("project.forget")}
              </button>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}

/** Freelance mode: the price to announce to the client (only when the project has a client). */
function ClientPrice({ taskId, quoteKey }: { taskId: string; quoteKey: string }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const [cq, setCq] = useState<ClientQuote | null>(null);
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    let cancelled = false;
    api
      .freelanceQuote(taskId)
      .then((r) => !cancelled && setCq(r))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [taskId, quoteKey]);
  if (!cq) return null;
  const fmt = (v: number) => amount(v, cq.currency, lang);
  const text = t("freelance.clientPrice", { likely: fmt(cq.total_likely), low: fmt(cq.total_low), high: fmt(cq.total_high) });
  return (
    <div className="client-price">
      <div className="grow">
        <span className="muted small">{t("freelance.clientPriceLabel")}</span>
        <div>
          <strong className="num">{fmt(cq.total_likely)}</strong>{" "}
          <span className="muted small num">{t("task.quoteRange", { low: fmt(cq.total_low), high: fmt(cq.total_high) })}</span>
        </div>
        <span className="muted small">{t("freelance.clientPriceDetail", { time: minutes(cq.human_minutes, lang), labour: fmt(cq.labour), ai: fmt(cq.ai_likely) })}</span>
      </div>
      <button className="btn-ghost btn-small" onClick={() => void navigator.clipboard.writeText(text).then(() => setCopied(true))}>
        {copied ? `✓ ${t("freelance.copied")}` : t("freelance.copyPrice")}
      </button>
    </div>
  );
}

/** Puts a prepared task in tonight's queue instead of running it now. */
function NightButton({ task, status, disabled }: { task: TaskRecord; status: AppStatus; disabled: boolean }) {
  const { t } = useTranslation();
  const [state, setState] = useState<"idle" | "queued" | "error">("idle");
  const community = status.plan === "community";
  if (state === "queued") return <span className="ok-text small">✓ {t("night.queued")}</span>;
  return (
    <>
      {state === "error" && <span className="bad-text small">{t("errors.unknown")}</span>}
      <button
        className="btn-ghost"
        disabled={disabled || community}
        title={community ? t("settings.proOnly") : t("night.addHint")}
        onClick={async () => {
          try {
            await api.nightEnqueue(task.id);
            setState("queued");
          } catch {
            setState("error");
          }
        }}
      >
        <Moon /> {t("night.add")} {community && <em className="badge">{t("settings.proOnly")}</em>}
      </button>
    </>
  );
}

function QuoteStage({
  task,
  status,
  busy,
  expert,
  promptDraft,
  setPromptDraft,
  onRun,
  onFullPrep,
  onEdit,
  onOverride,
  onSavePrompt,
}: {
  task: TaskRecord;
  status: AppStatus;
  busy: Busy;
  expert: boolean;
  promptDraft: string;
  setPromptDraft: (v: string) => void;
  onRun: () => void;
  onFullPrep: () => void;
  onEdit: () => void;
  onOverride: (model: string, effort: Effort | null, maxTurns: number) => void;
  onSavePrompt: () => void;
}) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const s = status.settings;
  const q = task.quote!;
  const spec = task.spec!;
  const route = task.route!;
  const sub = task.mode === "subscription" && q.quota;
  const [model, setModel] = useState(route.model);
  const [effort, setEffort] = useState<Effort | "">(route.effort ?? "");
  const [turns, setTurns] = useState(route.max_turns);
  const cap = task.cap_check;
  const pctPerUsd = sub && q.likely_usd > 0 ? q.quota!.likely_pct / q.likely_usd : null;
  const fmtCap = (usd: number) => (pctPerUsd ? percent(usd * pctPerUsd, lang) : money(usd, s, lang));
  const capAmount = cap ? fmtCap(cap.status === "blocked" ? cap.remaining_usd : cap.effective_cap_usd ?? 0) : "";
  const blocked = cap?.status === "blocked";

  return (
    <div className="stack">
      <Card title={t("task.quoteTitle")} tone={blocked ? "bad" : cap?.status === "warn" ? "warn" : undefined}>
        <div className="quote">
          <div className="quote-main">
            <span className="muted small">{t("task.quoteLikely")}</span>
            <span className="quote-value">{sub ? t("task.quotaShare", { pct: percent(q.quota!.likely_pct, lang) }) : money(q.likely_usd, s, lang)}</span>
            <span className="muted">
              {sub
                ? t("task.quoteRange", { low: percent(q.quota!.low_pct, lang), high: percent(q.quota!.high_pct, lang) })
                : t("task.quoteRange", { low: money(q.low_usd, s, lang), high: money(q.high_usd, s, lang) })}
            </span>
            {sub && <span className="muted small">{t("task.quotaApi", { amount: money(q.likely_usd, s, lang) })}</span>}
          </div>
          <div className="quote-notes small muted">
            <p>{q.calibrated ? t("task.calibrated", { count: q.samples }) : t("task.estimateNote")}</p>
            {sub && !q.quota!.measured && <p>{t("task.estimateUnmeasured")}</p>}
          </div>
        </div>
        {spec.express && <ExpressNote spec={spec} busy={busy} onFullPrep={onFullPrep} />}
        {cap && !(cap.status === "ok" && cap.effective_cap_usd === null) && (
          <p className={`cap cap-${cap.status}`}>
            {cap.status === "ok" ? t("task.capOk", { cap: capAmount }) : cap.status === "warn" ? t("task.capWarn", { cap: capAmount }) : t("task.capBlocked", { cap: capAmount })}
          </p>
        )}
        <ClientPrice taskId={task.id} quoteKey={`${q.likely_usd}`} />
        <div className="row between">
          <button className="btn-ghost" onClick={onEdit}>
            {t("task.edit")}
          </button>
          <div className="row">
            <NightButton task={task} status={status} disabled={blocked || busy !== null} />
            <button className="btn btn-big" disabled={blocked || busy !== null} onClick={onRun}>
              {t("task.run")}
            </button>
          </div>
        </div>
      </Card>

      <Card title={t("task.briefTitle")}>
        <h3 className="spec-title">{spec.title}</h3>
        <p>{spec.objective}</p>
        <h4>{t("task.criteriaTitle")}</h4>
        <ul className="criteria">
          {spec.success_criteria.map((c) => (
            <li key={c.id}>
              <span className="kind">{t(`task.kind.${c.check.kind}`)}</span>
              {c.description}
            </li>
          ))}
        </ul>
      </Card>

      {expert && (
        <Card title={t("task.routeTitle")}>
          <div className="grid-3">
            <label className="field">
              <span>{t("task.model")}</span>
              <select value={model} onChange={(e) => setModel(e.target.value)}>
                {[...new Set([route.model, ...MODELS])].map((m) => (
                  <option key={m} value={m}>
                    {m === "default" ? "auto" : m}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span>{t("task.effort")}</span>
              <select value={effort} onChange={(e) => setEffort(e.target.value as Effort | "")}>
                {EFFORTS.map((e) => (
                  <option key={e || "auto"} value={e}>
                    {e || "auto"}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span>{t("task.maxTurns")}</span>
              <input type="number" min={1} max={1000} value={turns} onChange={(e) => setTurns(Number(e.target.value))} />
            </label>
          </div>
          <div className="row end">
            <button className="btn-ghost" onClick={() => onOverride(model, effort || null, turns)}>
              {t("task.recompute")}
            </button>
          </div>
          <label className="field">
            <span>{t("task.prompt")}</span>
            <textarea className="mono prompt" rows={14} value={promptDraft} onChange={(e) => setPromptDraft(e.target.value)} />
          </label>
          <div className="row end">
            <button className="btn-ghost" disabled={promptDraft === task.prompt} onClick={onSavePrompt}>
              {t("task.savePrompt")}
            </button>
          </div>
        </Card>
      )}
    </div>
  );
}

function CostBar({ task, status, spent }: { task: TaskRecord; status: AppStatus; spent: number }) {
  const { t, i18n } = useTranslation();
  const cap = task.cap_check?.status === "ok" || task.cap_check?.status === "warn" ? task.cap_check.effective_cap_usd : null;
  const ratio = cap ? Math.min(1, spent / cap) : task.quote ? Math.min(1, spent / Math.max(task.quote.high_usd, 0.01)) : 0;
  const q = task.quote;
  const pctPerUsd = task.mode === "subscription" && q?.quota && q.likely_usd > 0 ? q.quota.likely_pct / q.likely_usd : null;
  const fmt = (usd: number) => (pctPerUsd ? percent(usd * pctPerUsd, i18n.language) : money(usd, status.settings, i18n.language));
  return (
    <div className="costbar">
      <div className="row between small">
        <span>{t("task.spentSoFar")}</span>
        <b>
          {fmt(spent)}
          {cap ? ` / ${fmt(cap)}` : ""}
        </b>
      </div>
      <div className="bar" role="progressbar" aria-valuenow={Math.round(ratio * 100)} aria-valuemin={0} aria-valuemax={100}>
        <span style={{ width: `${ratio * 100}%` }} className={ratio > 0.85 ? "hot" : ""} />
      </div>
    </div>
  );
}

function EventFeed({ events }: { events: LiveEvent[] }) {
  const { t, i18n } = useTranslation();
  const lines = useMemo(
    () =>
      events.flatMap((e, i) => {
        const ev = e.event;
        switch (ev.kind) {
          case "tool_use": {
            if (ev.name === "StructuredOutput") return [];
            const known = i18n.exists(`event.${ev.name}`);
            return [{ i, kind: "tool", text: known ? t(`event.${ev.name}`, { target: ev.summary }) : t("event.other", { name: ev.name }) }];
          }
          case "text":
            return [{ i, kind: "text", text: ev.text }];
          case "rate_limit":
            return ev.info.status === "rejected" && ev.info.resets_at
              ? [{ i, kind: "warn", text: t("event.rateLimited", { time: new Date(ev.info.resets_at * 1000).toLocaleTimeString(i18n.language) }) }]
              : [];
          case "retry":
            return [{ i, kind: "warn", text: t("event.retrying") }];
          default:
            return [];
        }
      }),
    [events, t, i18n],
  );
  return (
    <ol className="feed" aria-live="polite">
      {lines.map((l) => (
        <li key={l.i} className={`feed-${l.kind}`}>
          {l.text}
        </li>
      ))}
    </ol>
  );
}
