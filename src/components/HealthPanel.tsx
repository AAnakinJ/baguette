import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, openUrl } from "../lib/api";
import { useEvent } from "../lib/useEvent";
import type { DoctorItem, Fix } from "../lib/types";
import { Card, ErrorBanner, Spinner } from "../lib/ui";

const ICON = { ok: "✓", warn: "!", error: "✗" } as const;

export function HealthPanel({ refresh, goSettings }: { refresh: () => Promise<void>; goSettings: () => void }) {
  const { t } = useTranslation();
  const [items, setItems] = useState<DoctorItem[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [log, setLog] = useState<string[]>([]);
  const [confirm, setConfirm] = useState<Fix | null>(null);
  const [error, setError] = useState<unknown>(null);

  const run = useCallback(async () => {
    setItems(null);
    try {
      setItems(await api.doctor());
    } catch (e) {
      setError(e);
    }
  }, []);

  useEffect(() => {
    void run();
  }, [run]);
  useEvent<string>("setup-log", (l) => setLog((x) => [...x.slice(-300), l]));
  useEvent<boolean>("login-finished", () => void run());

  async function install(tool: "claude_code" | "git", update: boolean, key: string) {
    setConfirm(null);
    setLog([]);
    setBusy(key);
    try {
      const ok = await api.setupRun(tool, update);
      setLog((l) => [...l, ok ? t("health.done") : t("health.failed")]);
      await run();
      await refresh();
    } catch (e) {
      setError(e);
    } finally {
      setBusy(null);
    }
  }

  async function apply(f: Fix) {
    switch (f.kind) {
      case "install_claude":
        return install("claude_code", false, f.kind);
      case "update_claude":
        return install("claude_code", true, f.kind);
      case "install_git":
      case "remove_mcp":
        return setConfirm(f);
      case "sign_in":
        try {
          await api.startLogin();
        } catch (e) {
          setError(e);
        }
        return;
      case "add_api_key":
        return goSettings();
      case "open_url":
        return openUrl(f.url);
    }
  }

  async function confirmed() {
    const f = confirm;
    if (!f) return;
    if (f.kind === "install_git") return install("git", false, f.kind);
    if (f.kind === "remove_mcp") {
      setConfirm(null);
      try {
        await api.mcpRemove(f.name);
        await run();
      } catch (e) {
        setError(e);
      }
    }
  }

  const label = (it: DoctorItem) => (it.id === "mcp" ? it.name : t(`health.label.${it.id}`, { defaultValue: it.name }));
  const allOk = items?.every((i) => i.state === "ok");

  return (
    <div className="stack narrow">
      <ErrorBanner error={error} t={t} onClose={() => setError(null)} />
      <Card
        title={t("health.title")}
        actions={
          <button className="btn-ghost" disabled={items === null || busy !== null} onClick={run}>
            ↻ {t("health.recheck")}
          </button>
        }
      >
        <p className="muted small">{t("health.intro")}</p>
        {items === null ? (
          <Spinner label={t("health.checking")} />
        ) : (
          <ul className="report">
            {items.map((it, i) => (
              <li key={`${it.id}-${it.name}-${i}`} className={`report-item st-${it.state === "ok" ? "passed" : it.state === "warn" ? "manual" : "failed"}`}>
                <span className="icon" aria-hidden="true">
                  {ICON[it.state]}
                </span>
                <div className="grow">
                  <div className="row between">
                    <strong>{label(it)}</strong>
                    {it.id === "mcp" && <span className="tag">MCP</span>}
                  </div>
                  <span className="muted small">{t(it.detail, { defaultValue: it.detail })}</span>
                  {it.fixes.length > 0 && (
                    <div className="row wrap" style={{ marginTop: "0.4rem" }}>
                      {it.fixes.map((f, j) => (
                        <button key={j} className="btn-ghost btn-small" disabled={busy !== null} onClick={() => apply(f)}>
                          {t(`health.fix.${f.kind}`)}
                          {f.kind === "open_url" ? " ↗" : ""}
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              </li>
            ))}
          </ul>
        )}
        {allOk && <p className="ok-text small">✓ {t("health.allOk")}</p>}
      </Card>

      {confirm && (
        <Card tone="warn">
          <p>
            {confirm.kind === "install_git"
              ? t(navigator.platform.toLowerCase().includes("win") ? "setup.gitLicense" : "setup.adminPrompt")
              : confirm.kind === "remove_mcp"
                ? t("health.confirmRemove", { name: confirm.name })
                : ""}
          </p>
          <div className="row end">
            <button className="btn-ghost" onClick={() => setConfirm(null)}>
              {t("health.cancel")}
            </button>
            <button className="btn" onClick={confirmed}>
              {t("health.confirm")}
            </button>
          </div>
        </Card>
      )}

      {(busy || log.length > 0) && (
        <Card title={t("health.log")}>
          {busy && <Spinner label={t("health.installing")} />}
          <ol className="feed">
            {log.map((l, i) => (
              <li key={i} className="feed-tool">
                {l}
              </li>
            ))}
          </ol>
        </Card>
      )}
    </div>
  );
}
