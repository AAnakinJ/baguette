import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { AppStatus } from "../lib/types";
import { checkUpdate, type UpdateInfo } from "../lib/update";
import { Card } from "../lib/ui";

function useInstall(update: UpdateInfo | null) {
  const [pct, setPct] = useState<number | null | undefined>(undefined);
  const [failed, setFailed] = useState(false);
  const install = async () => {
    if (!update) return;
    setFailed(false);
    setPct(null);
    try {
      await update.install(setPct);
    } catch {
      setFailed(true);
      setPct(undefined);
    }
  };
  return { installing: pct !== undefined, pct, failed, install };
}

/** Top banner when a newer version exists (checked once at start-up, silently). */
export function UpdateBanner({ status }: { status: AppStatus }) {
  const { t } = useTranslation();
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [hidden, setHidden] = useState(false);
  const { installing, pct, failed, install } = useInstall(update);
  useEffect(() => {
    let stale = false;
    checkUpdate()
      .then((u) => !stale && setUpdate(u))
      .catch(() => undefined);
    return () => {
      stale = true;
    };
  }, []);
  if (!update || hidden) return null;
  return (
    <div className="update-banner" role="status">
      <span>{t("update.available", { product: status.product, version: update.version })}</span>
      {failed && <span className="bad-text small">{t("update.error")}</span>}
      <span className="row">
        {installing ? (
          <span className="small num">{t("update.installing", { pct: pct ?? "…" })}</span>
        ) : (
          <>
            <button className="btn-ghost btn-small" onClick={() => setHidden(true)}>
              {t("update.later")}
            </button>
            <button className="btn btn-small" onClick={install}>
              {t("update.install")}
            </button>
          </>
        )}
      </span>
    </div>
  );
}

/** Settings → Updates: current version and a manual check. */
export function UpdateCard({ status }: { status: AppStatus }) {
  const { t } = useTranslation();
  const [state, setState] = useState<"idle" | "checking" | "none" | "found" | "error">("idle");
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const { installing, pct, failed, install } = useInstall(update);
  async function check() {
    setState("checking");
    try {
      const u = await checkUpdate();
      setUpdate(u);
      setState(u ? "found" : "none");
    } catch {
      setState("error");
    }
  }
  return (
    <Card title={t("update.title")}>
      <div className="row between wrap">
        <span className="small">{t("update.current", { version: status.version })}</span>
        {state === "found" && update ? (
          installing ? (
            <span className="small num">{t("update.installing", { pct: pct ?? "…" })}</span>
          ) : (
            <button className="btn btn-small" onClick={install}>
              {t("update.installVersion", { version: update.version })}
            </button>
          )
        ) : (
          <button className="btn-ghost btn-small" disabled={state === "checking"} onClick={check}>
            {state === "checking" ? t("update.checking") : t("update.check")}
          </button>
        )}
      </div>
      {state === "none" && <p className="ok-text small">✓ {t("update.upToDate")}</p>}
      {(state === "error" || failed) && <p className="bad-text small">{t("update.error")}</p>}
      <p className="muted small">{t("update.signed")}</p>
    </Card>
  );
}
