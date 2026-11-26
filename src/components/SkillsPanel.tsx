import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api, pickFolder } from "../lib/api";
import type { AppStatus, ProjectView, ScanReport, Settings, SkillPolicy } from "../lib/types";
import { Card, ErrorBanner, Segmented, Spinner } from "../lib/ui";

const MIN: Record<SkillPolicy, number> = { strict: 90, standard: 70, open: 40 };

export function SkillsPanel({ status, project, save }: { status: AppStatus; project: ProjectView | null; save: (s: Settings) => Promise<void> }) {
  const { t } = useTranslation();
  const s = status.settings;
  const [path, setPath] = useState<string | null>(null);
  const [report, setReport] = useState<ScanReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);
  const allowed = !!report && report.has_skill_md && report.score >= MIN[s.skill_policy];

  async function pick() {
    const p = await pickFolder();
    if (!p) return;
    setPath(p);
    setReport(null);
    setDone(null);
    setBusy(true);
    try {
      setReport(await api.skillScan(p));
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  }

  async function install(projectId: string | null) {
    if (!path) return;
    try {
      const where = await api.skillInstall(path, projectId);
      setDone(t("skills.installed", { path: where }));
    } catch (e) {
      setError(e);
    }
  }

  return (
    <div className="stack narrow">
      <ErrorBanner error={error} t={t} onClose={() => setError(null)} />
      <Card title={t("skills.title")}>
        <p className="muted small">{t("skills.intro")}</p>
        <div className="field">
          <span>{t("skills.policy")}</span>
          <Segmented
            label={t("skills.policy")}
            value={s.skill_policy}
            options={[
              { value: "strict", label: t("skills.policyStrict") },
              { value: "standard", label: t("skills.policyStandard") },
              { value: "open", label: t("skills.policyOpen") },
            ]}
            onChange={(v) => save({ ...s, skill_policy: v })}
          />
        </div>
        <div className="row">
          <button className="btn" onClick={pick} disabled={busy}>
            {t("skills.pick")}
          </button>
          {path && <span className="muted mono small">{path}</span>}
        </div>
        {busy && <Spinner label={t("skills.scanning")} />}
      </Card>

      {report && (
        <Card
          title={report.name}
          tone={report.verdict === "trusted" ? "ok" : report.verdict === "caution" ? "warn" : "bad"}
          actions={<span className="tag">{t("skills.files", { count: report.files })}</span>}
        >
          {report.description && <p className="muted">{report.description}</p>}
          <div className="quote">
            <div className="quote-main">
              <span className="muted small">{t("skills.score")}</span>
              <span className="quote-value">{report.score}/100</span>
              <span className={report.verdict === "trusted" ? "ok-text" : report.verdict === "caution" ? "warn-text" : "bad-text"}>
                {t(`skills.verdict.${report.verdict}`)}
              </span>
            </div>
          </div>
          {!report.has_skill_md && <p className="bad-text small">{t("skills.noSkillMd")}</p>}
          {report.findings.length === 0 ? (
            <p className="ok-text small">✓ {t("skills.none")}</p>
          ) : (
            <ul className="report">
              {report.findings.map((f, i) => (
                <li key={i} className={`report-item st-${f.severity === "high" ? "failed" : "manual"}`}>
                  <span className="icon" aria-hidden="true">
                    {f.severity === "high" ? "✗" : "!"}
                  </span>
                  <div className="grow">
                    <div className="row between">
                      <span>{t(`skills.finding.${f.kind}`, { defaultValue: f.kind })}</span>
                      <span className="tag">{f.severity}</span>
                    </div>
                    <code className="small">
                      {f.path}
                      {f.line ? `:${f.line}` : ""}
                    </code>
                    {f.excerpt && <pre className="evidence">{f.excerpt}</pre>}
                  </div>
                </li>
              ))}
            </ul>
          )}
          {!allowed && report.has_skill_md && <p className="warn-text small">{t("skills.blocked", { min: MIN[s.skill_policy] })}</p>}
          <div className="row end wrap">
            <button className="btn-ghost" disabled={!allowed} onClick={() => install(null)}>
              {t("skills.installUser")}
            </button>
            <button className="btn" disabled={!allowed || !project} onClick={() => install(project!.project.id)}>
              {t("skills.installProject")}
            </button>
          </div>
          {done && <p className="ok-text small">✓ {done}</p>}
        </Card>
      )}
    </div>
  );
}
