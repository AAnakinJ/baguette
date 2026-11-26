// Calls into the Rust backend. Outside Tauri (plain browser preview, UI tests) a mock
// backend answers instead, so every screen can be shown without Claude Code.
import type {
  Answer,
  ApiError,
  AppStatus,
  DoctorItem,
  ScanReport,
  Snapshot,
  LiveEvent,
  ClientQuote,
  FreelanceProfile,
  LicenceView,
  RefreshRequest,
  Statement,
  MemoryItem,
  NightOptions,
  NightReport,
  NightView,
  Project,
  ProjectView,
  RouteOverrides,
  Settings,
  TaskRecord,
} from "./types";
import { mock } from "./mock";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) return mock.invoke<T>(cmd, args ?? {});
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

export function isApiError(e: unknown): e is ApiError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}

export async function on<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  if (!inTauri) return mock.listen<T>(event, handler);
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(event, (e) => handler(e.payload));
}

export async function pickFolder(): Promise<string | null> {
  if (!inTauri) return mock.pickFolder();
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ directory: true, multiple: false });
  return typeof r === "string" ? r : null;
}

export async function openUrl(url: string): Promise<void> {
  if (!inTauri) {
    window.open(url, "_blank", "noopener");
    return;
  }
  const { openUrl: open } = await import("@tauri-apps/plugin-opener");
  await open(url);
}

/** Asks where to save a GIF, then writes it. Returns false when the user cancels. */
export async function saveGif(bytes: Uint8Array, name: string): Promise<boolean> {
  if (!inTauri) {
    const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: "image/gif" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = name;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    return true;
  }
  const { save } = await import("@tauri-apps/plugin-dialog");
  const path = await save({ defaultPath: name, filters: [{ name: "GIF", extensions: ["gif"] }] });
  if (!path) return false;
  await call<void>("demo_save", { path, data: Array.from(bytes) });
  return true;
}

export const api = {
  status: () => call<AppStatus>("app_status"),
  saveSettings: (settings: Settings) => call<Settings>("save_settings", { settings }),
  setApiKey: (key: string) => call<void>("set_api_key", { key }),
  clearApiKey: () => call<void>("clear_api_key"),
  startLogin: () => call<void>("start_login"),
  openProject: (path: string) => call<ProjectView>("open_project", { path }),
  listProjects: () => call<Project[]>("list_projects"),
  forgetMemory: (projectId: string, key: string) => call<MemoryItem[]>("forget_memory", { projectId, key }),
  taskStart: (projectId: string, request: string) => call<TaskRecord>("task_start", { projectId, request }),
  taskPlan: (taskId: string, answers: Answer[]) => call<TaskRecord>("task_plan", { taskId, answers }),
  taskOverride: (taskId: string, overrides: RouteOverrides) => call<TaskRecord>("task_override", { taskId, overrides }),
  taskEditPrompt: (taskId: string, prompt: string) => call<TaskRecord>("task_edit_prompt", { taskId, prompt }),
  taskGet: (taskId: string) => call<TaskRecord>("task_get", { taskId }),
  taskList: (projectId: string | null) => call<TaskRecord[]>("task_list", { projectId }),
  taskRun: (taskId: string) => call<TaskRecord>("task_run", { taskId }),
  taskRetry: (taskId: string) => call<TaskRecord>("task_retry", { taskId }),
  taskVerify: (taskId: string) => call<TaskRecord>("task_verify", { taskId }),
  taskAsk: (projectId: string, question: string) => call<TaskRecord>("task_ask", { projectId, question }),
  taskFollowUp: (taskId: string, request: string) => call<TaskRecord>("task_follow_up", { taskId, request }),
  taskCancel: (key: string) => call<boolean>("task_cancel", { key }),
  taskApproveCommand: (taskId: string, command: string) => call<TaskRecord>("task_approve_command", { taskId, command }),
  taskSetManual: (taskId: string, criterionId: string, passed: boolean) =>
    call<TaskRecord>("task_set_manual", { taskId, criterionId, passed }),
  // Phase 3
  doctor: () => call<DoctorItem[]>("doctor_run"),
  setupRun: (tool: "claude_code" | "git", update: boolean) => call<boolean>("setup_run", { tool, update }),
  mcpRemove: (name: string) => call<boolean>("mcp_remove", { name }),
  timelineList: (projectId: string) => call<Snapshot[]>("timeline_list", { projectId }),
  timelineRestore: (projectId: string, snapshotId: string) => call<string>("timeline_restore", { projectId, snapshotId }),
  skillScan: (path: string) => call<ScanReport>("skill_scan", { path }),
  skillInstall: (path: string, projectId: string | null) => call<string>("skill_install", { path, projectId }),
  // Phase 4
  taskScreenshot: (path: string) => call<string>("task_screenshot", { path }),
  nightGet: () => call<NightView>("night_get"),
  nightEnqueue: (taskId: string) => call<NightView>("night_enqueue", { taskId }),
  nightDequeue: (taskId: string) => call<NightView>("night_dequeue", { taskId }),
  nightMove: (taskId: string, delta: number) => call<NightView>("night_move", { taskId, delta }),
  nightStart: (options: NightOptions) => call<NightReport>("night_start", { options }),
  nightStop: () => call<boolean>("task_cancel", { key: "night" }),
  freelanceGet: (projectId: string) => call<FreelanceProfile | null>("freelance_get", { projectId }),
  freelanceSave: (projectId: string, profile: FreelanceProfile) => call<FreelanceProfile>("freelance_save", { projectId, profile }),
  freelanceQuote: (taskId: string) => call<ClientQuote | null>("freelance_quote", { taskId }),
  freelanceSetMinutes: (taskId: string, minutes: number) => call<void>("freelance_set_minutes", { taskId, minutes }),
  freelanceStatement: (projectId: string, from: number, to: number) => call<Statement>("freelance_statement", { projectId, from, to }),
  // Phase 5
  licenceGet: () => call<LicenceView>("licence_get"),
  licenceInstall: (token: string, key: string | null) => call<LicenceView>("licence_install", { token, key }),
  licenceRefreshRequest: () => call<RefreshRequest>("licence_refresh_request"),
  licenceTrialRefused: () => call<LicenceView>("licence_trial_refused"),
  licenceForget: (keepKey: boolean) => call<LicenceView>("licence_forget", { keepKey }),
  flagsApply: (token: string) => call<boolean>("flags_apply", { token }),
};

export type { LiveEvent };
