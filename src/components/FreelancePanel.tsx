import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../lib/api";
import type { AppStatus, FreelanceProfile, ProjectView, Statement } from "../lib/types";
import { amount, day, minutes } from "../lib/format";
import { Card, ErrorBanner, Spinner } from "../lib/ui";

const EMPTY: FreelanceProfile = {
  client_name: "",
  client_details: "",
  freelancer_name: "",
  hourly_rate: 50,
  bill_ai_costs: true,
  markup_pct: 0,
  review_minutes: 15,
  footer: "",
};

/** "2026-11" -> [start, end) in unix seconds, local time. */
function monthRange(month: string): [number, number] {
  const [y, m] = month.split("-").map(Number);
  return [new Date(y, m - 1, 1).getTime() / 1000, new Date(y, m, 1).getTime() / 1000];
}

function thisMonth(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
}

/** Freelance mode: the client of this project, and the month's activity statement. */
export function FreelancePanel({ project, status }: { project: ProjectView; status: AppStatus }) {
  const { t } = useTranslation();
  const community = status.plan === "community";
  const projectId = project.project.id;
  const [profile, setProfile] = useState<FreelanceProfile | null>(null);
  const [draft, setDraft] = useState<FreelanceProfile>(EMPTY);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<unknown>(null);

  useEffect(() => {
    api
      .freelanceGet(projectId)
      .then((p) => {
        setProfile(p);
        setDraft(p ?? { ...EMPTY, bill_ai_costs: status.settings.connection_mode === "api_key" });
      })
      .catch(setError);
  }, [projectId, status.settings.connection_mode]);

  const set = <K extends keyof FreelanceProfile>(k: K, v: FreelanceProfile[K]) => {
    setSaved(false);
    setDraft({ ...draft, [k]: v });
  };
  const cur = status.settings.currency === "eur" ? "€" : "$";

  return (
    <div className="stack narrow">
      <ErrorBanner error={error} t={t} onClose={() => setError(null)} />
      <Card
        title={t("freelance.title")}
        actions={
          <button
            className="btn"
            disabled={community}
            onClick={async () => {
              try {
                setProfile(await api.freelanceSave(projectId, draft));
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
        <p className="muted small">{t("freelance.intro")}</p>
        {community && <p className="warn-text small">{t("freelance.proOnly")}</p>}
        <div className="grid-2">
          <label className="field">
            <span>{t("freelance.clientName")}</span>
            <input value={draft.client_name} maxLength={200} onChange={(e) => set("client_name", e.target.value)} />
          </label>
          <label className="field">
            <span>{t("freelance.freelancerName")}</span>
            <input value={draft.freelancer_name} maxLength={200} onChange={(e) => set("freelancer_name", e.target.value)} />
          </label>
        </div>
        <label className="field">
          <span>{t("freelance.clientDetails")}</span>
          <textarea rows={2} value={draft.client_details} maxLength={1000} onChange={(e) => set("client_details", e.target.value)} />
        </label>
        <div className="grid-3">
          <label className="field">
            <span>{t("freelance.rate")}</span>
            <div className="row">
              <input type="number" min={0} step={5} value={draft.hourly_rate} onChange={(e) => set("hourly_rate", Math.max(0, Number(e.target.value) || 0))} />
              <span className="muted">{cur}/h</span>
            </div>
          </label>
          <label className="field">
            <span>{t("freelance.reviewMinutes")}</span>
            <div className="row">
              <input type="number" min={0} step={5} value={draft.review_minutes} onChange={(e) => set("review_minutes", Math.max(0, Math.round(Number(e.target.value) || 0)))} />
              <span className="muted">min</span>
            </div>
          </label>
          <label className="field">
            <span>{t("freelance.markup")}</span>
            <div className="row">
              <input type="number" min={0} step={5} disabled={!draft.bill_ai_costs} value={draft.markup_pct} onChange={(e) => set("markup_pct", Math.max(0, Number(e.target.value) || 0))} />
              <span className="muted">%</span>
            </div>
          </label>
        </div>
        <label className="toggle">
          <input type="checkbox" checked={draft.bill_ai_costs} onChange={(e) => set("bill_ai_costs", e.target.checked)} />
          <span>{t("freelance.billAi")}</span>
        </label>
        {status.settings.connection_mode === "subscription" && <small className="muted">{t("freelance.subscriptionNote")}</small>}
        <label className="field">
          <span>{t("freelance.footer")}</span>
          <textarea rows={2} value={draft.footer} maxLength={1000} placeholder={t("freelance.footerHint")} onChange={(e) => set("footer", e.target.value)} />
        </label>
        <small className="muted">{t("freelance.localOnly")}</small>
      </Card>

      {profile && !community && <StatementCard projectId={projectId} setError={setError} />}
    </div>
  );
}

function StatementCard({ projectId, setError }: { projectId: string; setError: (e: unknown) => void }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const [month, setMonth] = useState(thisMonth());
  const [st, setSt] = useState<Statement | null>(null);
  const [copied, setCopied] = useState(false);

  const load = useCallback(async () => {
    try {
      const [from, to] = monthRange(month);
      setSt(await api.freelanceStatement(projectId, from, to));
    } catch (e) {
      setError(e);
    }
  }, [month, projectId, setError]);
  useEffect(() => {
    void load();
  }, [load]);

  if (!st) return <Spinner label={t("app.loading")} />;
  const money = (v: number) => amount(v, st.currency, lang);
  const p = st.profile;
  const period = new Intl.DateTimeFormat(lang === "pt" ? "pt-BR" : lang, { month: "long", year: "numeric" }).format(new Date(st.from * 1000));

  function copy() {
    if (!st) return;
    const head = [t("freelance.col.date"), t("freelance.col.task"), t("freelance.col.proof"), t("freelance.col.time"), t("freelance.col.labour"), t("freelance.col.ai"), t("freelance.col.total")];
    const rows = st.lines.map((l) => [day(l.finished_at, lang), l.title, `${l.criteria_passed}/${l.criteria_total}`, String(l.human_minutes), l.labour.toFixed(2), l.ai_billed.toFixed(2), l.total.toFixed(2)]);
    const text = [head, ...rows, ["", t("freelance.total"), "", String(st.human_minutes), st.labour.toFixed(2), st.ai_billed.toFixed(2), st.total.toFixed(2)]].map((r) => r.join("\t")).join("\n");
    void navigator.clipboard.writeText(text).then(() => setCopied(true));
  }

  return (
    <Card
      title={t("freelance.statementTitle")}
      actions={
        <div className="row no-print">
          <input type="month" value={month} onChange={(e) => e.target.value && setMonth(e.target.value)} aria-label={t("freelance.month")} />
        </div>
      }
    >
      <p className="muted small no-print">{t("freelance.statementHint")}</p>
      <div className="printable statement">
        <header className="statement-head">
          <div>
            <div className="statement-from">{p.freelancer_name}</div>
            <div className="muted small">{t("freelance.statementFor", { period, project: st.project_name })}</div>
          </div>
          <div className="statement-to">
            <div>{p.client_name}</div>
            {p.client_details && <div className="small prewrap">{p.client_details}</div>}
          </div>
        </header>
        {st.lines.length === 0 ? (
          <p className="muted small">{t("freelance.noWork")}</p>
        ) : (
          <div className="table-wrap">
            <table className="statement-table">
              <thead>
                <tr>
                  <th>{t("freelance.col.date")}</th>
                  <th>{t("freelance.col.task")}</th>
                  <th>{t("freelance.col.proof")}</th>
                  <th className="r">{t("freelance.col.time")}</th>
                  <th className="r">{t("freelance.col.labour")}</th>
                  {p.bill_ai_costs && <th className="r">{t("freelance.col.ai")}</th>}
                  <th className="r">{t("freelance.col.total")}</th>
                </tr>
              </thead>
              <tbody>
                {st.lines.map((l) => (
                  <tr key={l.task_id}>
                    <td className="num nowrap">{day(l.finished_at, lang)}</td>
                    <td>{l.title}</td>
                    <td className="num nowrap">{l.criteria_total > 0 ? `✓ ${l.criteria_passed}/${l.criteria_total}` : "—"}</td>
                    <td className="r num nowrap">
                      <MinutesInput
                        value={l.human_minutes}
                        label={minutes(l.human_minutes, lang)}
                        onSave={async (m) => {
                          try {
                            await api.freelanceSetMinutes(l.task_id, m);
                            await load();
                          } catch (e) {
                            setError(e);
                          }
                        }}
                      />
                    </td>
                    <td className="r num">{money(l.labour)}</td>
                    {p.bill_ai_costs && <td className="r num">{money(l.ai_billed)}</td>}
                    <td className="r num">{money(l.total)}</td>
                  </tr>
                ))}
              </tbody>
              <tfoot>
                <tr>
                  <td colSpan={3}>{t("freelance.total")}</td>
                  <td className="r num">{minutes(st.human_minutes, lang)}</td>
                  <td className="r num">{money(st.labour)}</td>
                  {p.bill_ai_costs && <td className="r num">{money(st.ai_billed)}</td>}
                  <td className="r num strong">{money(st.total)}</td>
                </tr>
              </tfoot>
            </table>
          </div>
        )}
        <p className="small muted">{t("freelance.proofNote", { agent: minutes(st.agent_minutes, lang) })}</p>
        {p.footer && <p className="small prewrap">{p.footer}</p>}
        <p className="small muted print-only">{t("freelance.notInvoice")}</p>
      </div>
      <div className="row between wrap no-print statement-actions">
        <span className="muted small">{t("freelance.notInvoice")}</span>
        <div className="row">
          <button className="btn-ghost" disabled={st.lines.length === 0} onClick={copy}>
            {copied ? `✓ ${t("freelance.copied")}` : t("freelance.copy")}
          </button>
          <button className="btn" disabled={st.lines.length === 0} onClick={() => window.print()}>
            {t("freelance.print")}
          </button>
        </div>
      </div>
    </Card>
  );
}

/** Shows the time; click to type the real time spent on the task. */
function MinutesInput({ value, label, onSave }: { value: number; label: string; onSave: (m: number) => void }) {
  const { t } = useTranslation();
  const [edit, setEdit] = useState(false);
  const [v, setV] = useState(String(value));
  if (!edit)
    return (
      <button className="link" title={t("freelance.editTime")} onClick={() => setEdit(true)}>
        {label}
      </button>
    );
  return (
    <input
      className="minutes-input"
      type="number"
      min={0}
      autoFocus
      value={v}
      onChange={(e) => setV(e.target.value)}
      onBlur={() => {
        setEdit(false);
        const m = Math.max(0, Math.round(Number(v) || 0));
        if (m !== value) onSave(m);
      }}
      onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
    />
  );
}
