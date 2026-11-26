import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../lib/api";
import type { AppStatus, ProjectView, TaskRecord } from "../lib/types";
import { date, money } from "../lib/format";
import { Card, ErrorBanner } from "../lib/ui";
import { Timeline } from "./Timeline";

export function History({ project, status, onOpen }: { project: ProjectView; status: AppStatus; onOpen: (id: string) => void }) {
  const { t, i18n } = useTranslation();
  const [tasks, setTasks] = useState<TaskRecord[] | null>(null);
  const [error, setError] = useState<unknown>(null);

  useEffect(() => {
    api.taskList(project.project.id).then(setTasks).catch(setError);
  }, [project.project.id]);

  const total = (tasks ?? []).reduce((a, b) => a + b.total_cost_usd, 0);

  return (
    <div className="stack">
    <Card title={`${t("history.title")} · ${project.project.name}`} actions={tasks && tasks.length > 0 ? <span className="muted small">{money(total, status.settings, i18n.language)}</span> : null}>
      <ErrorBanner error={error} t={t} />
      {tasks && tasks.length === 0 && <p className="muted">{t("history.empty")}</p>}
      <ul className="history">
        {(tasks ?? []).map((tk) => (
          <li key={tk.id}>
            <button className="link-row" onClick={() => onOpen(tk.id)}>
              <span className="grow">
                <strong>{tk.spec?.title ?? tk.request}</strong>
                <span className="muted small">{date(tk.created_at, i18n.language)}</span>
              </span>
              <span className={`tag tag-${tk.status}`}>{t(`history.status.${tk.status}`)}</span>
              <span className="mono small">{money(tk.total_cost_usd, status.settings, i18n.language)}</span>
            </button>
          </li>
        ))}
      </ul>
    </Card>
    <Timeline projectId={project.project.id} />
    </div>
  );
}
