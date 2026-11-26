// Drives the REAL desktop app (Tauri dev build) through the WebView2 debug port and calls its
// Rust commands from inside the window, like the interface does. No Claude tokens are spent.
//   1. $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9229"; npm run tauri dev
//   2. node scripts/e2e-app.mjs [screenshot.png]
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const PORT = Number(process.env.CDP_PORT ?? 9229);
const shotPath = process.argv[2];

async function target() {
  for (let i = 0; i < 360; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${PORT}/json`)).json();
      const page = list.find((t) => t.type === "page" && /localhost:5173|tauri\.localhost/.test(t.url));
      if (page) return page;
    } catch {
      /* app not started yet */
    }
    await new Promise((r) => setTimeout(r, 1000));
  }
  throw new Error("the app window did not appear");
}

const page = await target();
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((ok, ko) => ((ws.onopen = ok), (ws.onerror = ko)));
let seq = 0;
const pending = new Map();
ws.onmessage = (m) => {
  const msg = JSON.parse(m.data);
  if (msg.id && pending.has(msg.id)) {
    pending.get(msg.id)(msg);
    pending.delete(msg.id);
  }
};
const cdp = (method, params = {}) =>
  new Promise((resolve) => {
    const id = ++seq;
    pending.set(id, resolve);
    ws.send(JSON.stringify({ id, method, params }));
  });
async function js(expression) {
  const r = await cdp("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (r.result?.exceptionDetails) return { error: r.result.exceptionDetails.exception?.description ?? r.result.exceptionDetails.text };
  return r.result?.result?.value;
}
const invoke = (cmd, args = {}) => js(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)}).then(v => ({ ok: v }), e => ({ err: e }))`);

// Wait for the interface to be ready.
for (let i = 0; i < 60 && !(await js("document.readyState === 'complete' && !!window.__TAURI_INTERNALS__")); i++) await new Promise((r) => setTimeout(r, 500));

const results = [];
const check = (name, ok, detail = "") => results.push({ name, ok: !!ok, detail });

const status = (await invoke("app_status")).ok;
check("app_status", status?.product === "Baguette", `plan=${status?.plan} early=${status?.early_access} claude=${status?.claude?.version}`);
check("early access: every feature on", status?.early_access === true && status?.plan === "pro");

const lic = (await invoke("licence_get")).ok;
check("licence_get (no network in early access)", lic?.early_access === true && lic?.status?.plan === "pro");

const settings = { ...status.settings, onboarded: true, language: "fr" };
const saved = await invoke("save_settings", { settings });
check("save_settings", saved.ok?.onboarded === true, saved.err ? JSON.stringify(saved.err) : "");

const dir = mkdtempSync(join(tmpdir(), "baguette-e2e-"));
writeFileSync(join(dir, "index.html"), "<!doctype html><title>Test</title><h1>Bonjour</h1>");
const project = await invoke("open_project", { path: dir });
check("open_project", project.ok?.project?.name?.startsWith("baguette-e2e-"), project.err ? JSON.stringify(project.err) : `stats=${JSON.stringify(project.ok?.stats ?? {}).slice(0, 80)}`);
const pid = project.ok?.project?.id;

const doctor = await invoke("doctor_run");
check("doctor_run", Array.isArray(doctor.ok) && doctor.ok.some((i) => i.id === "claude"), doctor.ok?.map((i) => `${i.id}:${i.state}`).join(" "));

const night = await invoke("night_get");
check("night_get", Array.isArray(night.ok?.queue), night.err ? JSON.stringify(night.err) : "");

const fl = await invoke("freelance_save", { projectId: pid, profile: { client_name: "Client test", client_details: "", freelancer_name: "", hourly_rate: 50, bill_ai_costs: false, markup_pct: 0, review_minutes: 15, footer: "" } });
check("freelance_save (Pro free in early access)", fl.ok?.client_name === "Client test", fl.err ? JSON.stringify(fl.err) : "");

const tasks = await invoke("task_list", { projectId: pid });
check("task_list", Array.isArray(tasks.ok), tasks.err ? JSON.stringify(tasks.err) : "");

const tl = await invoke("timeline_list", { projectId: pid });
check("timeline_list", Array.isArray(tl.ok), tl.err ? JSON.stringify(tl.err) : "");

const scan = await invoke("skill_scan", { path: dir });
check("skill_scan", typeof scan.ok?.score === "number", scan.err ? JSON.stringify(scan.err) : `score=${scan.ok?.score}`);

// The updater answers (with an error while no release exists) instead of crashing the app.
const upd = await js("window.__TAURI_INTERNALS__.invoke('plugin:updater|check', {}).then(v => ({ ok: v }), e => ({ err: String(e) }))");
check("updater plugin answers", upd && (upd.ok !== undefined || typeof upd.err === "string"), upd?.err ? upd.err.slice(0, 90) : "update found");

// Reload so the interface shows the saved settings, then a screenshot of the real window.
await js("location.reload()");
await new Promise((r) => setTimeout(r, 3000));
const text = await js("document.body.innerText.slice(0, 400)");
check("interface renders in French", /Nouvelle tâche|Projets|Accès anticipé/.test(text ?? ""), (text ?? "").replace(/\s+/g, " ").slice(0, 120));
if (shotPath) {
  const shot = await cdp("Page.captureScreenshot", { format: "png" });
  if (shot.result?.data) writeFileSync(shotPath, Buffer.from(shot.result.data, "base64"));
}

for (const r of results) console.log(`${r.ok ? "OK " : "NO "} ${r.name}${r.detail ? `  — ${r.detail}` : ""}`);
console.log(`\n${results.filter((r) => r.ok).length}/${results.length} checks passed`);
ws.close();
process.exit(results.every((r) => r.ok) ? 0 : 1);
