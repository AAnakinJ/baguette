import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, openUrl, pickFolder } from "./lib/api";
import { feedbackUrl } from "./lib/feedback";
import { syncLicence } from "./lib/licence";
import type { AppStatus, Project, ProjectView, Settings } from "./lib/types";
import { applyLanguage } from "./i18n";
import { ErrorBanner, Spinner } from "./lib/ui";
import { Header } from "./components/Header";
import { Onboarding } from "./components/Onboarding";
import { ProjectPicker } from "./components/ProjectPicker";
import { TaskFlow } from "./components/TaskFlow";
import { History } from "./components/History";
import { SettingsPanel } from "./components/SettingsPanel";
import { HealthPanel } from "./components/HealthPanel";
import { SkillsPanel } from "./components/SkillsPanel";
import { Moon } from "./components/Logo";
import { NightPanel } from "./components/NightPanel";
import { FreelancePanel } from "./components/FreelancePanel";
import { UpdateBanner } from "./components/Updates";

type View = "task" | "history" | "client" | "night" | "settings" | "health" | "skills";

export default function App() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [projects, setProjects] = useState<Project[]>([]);
  const [project, setProject] = useState<ProjectView | null>(null);
  const [view, setView] = useState<View>("task");
  const [openTaskId, setOpenTaskId] = useState<string | null>(null);
  const [taskKey, setTaskKey] = useState(0);
  const [error, setError] = useState<unknown>(null);

  const refresh = useCallback(async () => {
    try {
      const s = await api.status();
      setStatus(s);
      applyLanguage(s.settings.language);
    } catch (e) {
      setError(e);
    }
  }, []);

  useEffect(() => {
    void refresh();
    api.listProjects().then(setProjects).catch(setError);
  }, [refresh]);

  // Licence and remote flags: trial, renewal, kill switch. Silent when offline.
  useEffect(() => {
    let stale = false;
    api
      .licenceGet()
      .then(syncLicence)
      .then(() => (stale ? undefined : refresh()))
      .catch(() => undefined);
    return () => {
      stale = true;
    };
  }, [refresh]);

  const saveSettings = useCallback(
    async (next: Settings) => {
      await api.saveSettings(next);
      await refresh();
    },
    [refresh],
  );

  const openProject = useCallback(async (path: string) => {
    try {
      const v = await api.openProject(path);
      setProject(v);
      setProjects(await api.listProjects());
      setView("task");
      setOpenTaskId(null);
      setTaskKey((k) => k + 1);
    } catch (e) {
      setError(e);
    }
  }, []);

  const chooseFolder = useCallback(async () => {
    const path = await pickFolder();
    if (path) await openProject(path);
  }, [openProject]);

  if (!status) {
    return (
      <div className="center-screen">
        {error ? <ErrorBanner error={error} t={t} /> : <Spinner label={t("app.loading")} />}
      </div>
    );
  }

  if (!status.settings.onboarded) {
    return <Onboarding status={status} refresh={refresh} save={saveSettings} />;
  }

  return (
    <div className="shell">
      <Header status={status} save={saveSettings} onSettings={() => setView("settings")} />
      <aside className="sidebar">
        <nav aria-label={t("nav.projects")}>
          <h3>{t("nav.projects")}</h3>
          <ul className="project-list">
            {projects.map((p) => (
              <li key={p.id}>
                <button className={project?.project.id === p.id ? "active" : ""} onClick={() => openProject(p.path)} title={p.path}>
                  {p.name}
                </button>
              </li>
            ))}
          </ul>
          <button className="btn-ghost btn-block" onClick={chooseFolder}>
            + {t("nav.openFolder")}
          </button>
        </nav>
        <nav className="views">
          <button
            className={view === "task" ? "active" : ""}
            disabled={!project}
            onClick={() => {
              setView("task");
              setOpenTaskId(null);
              setTaskKey((k) => k + 1);
            }}
          >
            {t("nav.newTask")}
          </button>
          <button className={view === "history" ? "active" : ""} disabled={!project} onClick={() => setView("history")}>
            {t("nav.history")}
          </button>
          <button className={view === "client" ? "active" : ""} disabled={!project} onClick={() => setView("client")}>
            {t("nav.client")}
          </button>
          <button className={view === "night" ? "active" : ""} onClick={() => setView("night")}>
            <Moon /> {t("nav.night")}
          </button>
          <button className={view === "skills" ? "active" : ""} onClick={() => setView("skills")}>
            {t("nav.skills")}
          </button>
          <button className={view === "health" ? "active" : ""} onClick={() => setView("health")}>
            {t("nav.health")}
          </button>
          <button className={view === "settings" ? "active" : ""} onClick={() => setView("settings")}>
            {t("nav.settings")}
          </button>
        </nav>
        <button className="feedback-link" title={t("feedback.hint")} onClick={() => openUrl(feedbackUrl(status))}>
          {t("feedback.button")}
        </button>
        <p className="powered">{t("app.poweredBy")}</p>
      </aside>
      <main className="main">
        <UpdateBanner status={status} />
        <ErrorBanner error={error} t={t} onClose={() => setError(null)} />
        {view === "settings" ? (
          <SettingsPanel status={status} save={saveSettings} refresh={refresh} />
        ) : view === "health" ? (
          <HealthPanel refresh={refresh} goSettings={() => setView("settings")} />
        ) : view === "night" ? (
          <NightPanel
            status={status}
            onOpenTask={async (taskId, projectId) => {
              const p = projects.find((x) => x.id === projectId);
              if (p) await openProject(p.path);
              setOpenTaskId(taskId);
              setView("task");
              setTaskKey((k) => k + 1);
            }}
          />
        ) : view === "skills" ? (
          <SkillsPanel status={status} project={project} save={saveSettings} />
        ) : !project ? (
          <ProjectPicker projects={projects} onPick={chooseFolder} onOpen={openProject} />
        ) : view === "client" ? (
          <FreelancePanel key={project.project.id} project={project} status={status} />
        ) : view === "history" ? (
          <History
            project={project}
            status={status}
            onOpen={(id) => {
              setOpenTaskId(id);
              setView("task");
              setTaskKey((k) => k + 1);
            }}
          />
        ) : (
          <TaskFlow key={taskKey} project={project} status={status} taskId={openTaskId} onChanged={refresh} onNew={() => { setOpenTaskId(null); setTaskKey((k) => k + 1); }} />
        )}
      </main>
    </div>
  );
}
