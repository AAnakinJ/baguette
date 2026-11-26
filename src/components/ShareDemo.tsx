import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, saveGif } from "../lib/api";
import type { AppStatus, TaskRecord } from "../lib/types";
import { direction, duration, money, percent } from "../lib/format";
import { Card, Spinner } from "../lib/ui";

/** Turns a finished task into a looping animated GIF to share (post, README, client). */
export function ShareDemo({ task, status }: { task: TaskRecord; status: AppStatus }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const s = status.settings;
  const community = status.plan === "community";
  const [watermark, setWatermark] = useState(true);
  const [state, setState] = useState<"idle" | "making" | "ready" | "saved" | "error">("idle");
  const [gif, setGif] = useState<{ bytes: Uint8Array; url: string } | null>(null);

  useEffect(
    () => () => {
      if (gif) URL.revokeObjectURL(gif.url);
    },
    [gif],
  );

  async function make() {
    setState("making");
    try {
      const attempt = task.attempts[task.attempts.length - 1];
      const shots = await Promise.all(
        (attempt?.screenshots ?? []).map(async (sh) => ({ device: sh.device, url: await api.taskScreenshot(sh.path) })),
      );
      const report = task.report;
      const q = task.quote;
      const quote = q
        ? task.mode === "subscription" && q.quota
          ? t("demo.quoteQuota", { pct: percent(q.quota.likely_pct, lang) })
          : t("demo.quote", { amount: money(q.likely_usd, s, lang) })
        : "";
      const time = attempt && attempt.duration_ms > 0 ? duration(task.attempts.reduce((a, x) => a + x.duration_ms, 0), lang) : "";
      const { buildDemo } = await import("../lib/demo");
      const bytes = await buildDemo({
        product: status.product,
        watermark: watermark || community,
        rtl: direction(lang) === "rtl",
        request: task.request,
        title: task.spec?.title ?? "",
        quote,
        verdict: report ? t(`verdict.${report.verdict}`) : t("task.reportTitle"),
        verdictOk: report?.verdict === "success",
        stats: [money(task.total_cost_usd, s, lang), time].filter(Boolean).join(" · "),
        criteria: (report?.items ?? []).map((it) => ({ text: it.description, ok: it.status === "passed" ? true : it.status === "failed" ? false : null })),
        shots: shots.filter((x) => x.url),
        labels: {
          request: t("demo.request"),
          desktop: t("task.device.desktop"),
          mobile: t("task.device.mobile"),
          proof: t("demo.proof"),
          tagline: t("app.tagline"),
          criteria: report ? t("demo.criteria", { passed: report.passed, total: report.items.length }) : "",
        },
      });
      setGif({ bytes, url: URL.createObjectURL(new Blob([bytes as BlobPart], { type: "image/gif" })) });
      setState("ready");
    } catch {
      setState("error");
    }
  }

  return (
    <Card title={t("demo.title")}>
      <p className="muted small">{t("demo.hint")}</p>
      <label className="toggle">
        <input type="checkbox" disabled={community} checked={watermark || community} onChange={(e) => setWatermark(e.target.checked)} />
        <span>
          {t("demo.watermark", { product: status.product })} {community && <em className="badge">{t("demo.watermarkPro")}</em>}
        </span>
      </label>
      {gif && <img className="demo-preview" src={gif.url} alt={t("demo.title")} width={880} height={550} />}
      <div className="row end">
        {state === "making" && <Spinner label={t("demo.making")} />}
        {state === "error" && <span className="bad-text small">{t("errors.unknown")}</span>}
        {gif && <span className="muted small num">{t("demo.size", { kb: Math.round(gif.bytes.length / 1024) })}</span>}
        {gif && (
          <button
            className="btn-ghost"
            onClick={async () => {
              try {
                if (await saveGif(gif.bytes, `${(task.spec?.title ?? "demo").replace(/[^\p{L}\p{N}]+/gu, "-").slice(0, 40)}.gif`)) setState("saved");
              } catch {
                setState("error");
              }
            }}
          >
            {state === "saved" ? `✓ ${t("demo.saved")}` : t("demo.save")}
          </button>
        )}
        <button className="btn" disabled={state === "making"} onClick={make}>
          {gif ? t("demo.remake") : t("demo.make")}
        </button>
      </div>
    </Card>
  );
}
