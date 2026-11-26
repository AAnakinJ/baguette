import { useTranslation } from "react-i18next";
import type { Project } from "../lib/types";
import { Card } from "../lib/ui";

export function ProjectPicker({ projects, onPick, onOpen }: { projects: Project[]; onPick: () => void; onOpen: (path: string) => void }) {
  const { t } = useTranslation();
  return (
    <div className="narrow stack">
      <Card title={t("project.pickTitle")}>
        <p className="muted">{t("project.pickText")}</p>
        <button className="btn" onClick={onPick}>
          {t("project.pick")}
        </button>
      </Card>
      {projects.length > 0 && (
        <Card title={t("project.recent")}>
          <ul className="plain-list">
            {projects.map((p) => (
              <li key={p.id}>
                <button className="link-row" onClick={() => onOpen(p.path)}>
                  <strong>{p.name}</strong>
                  <span className="muted mono">{p.path}</span>
                </button>
              </li>
            ))}
          </ul>
        </Card>
      )}
    </div>
  );
}
