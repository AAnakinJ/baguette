// Mirrors the serde output of baguette-core. Keep in sync with the Rust types.

export type UiMode = "beginner" | "expert";
export type Autonomy = "careful" | "balanced" | "autopilot";
export type ConnectionMode = "api_key" | "subscription";
export type Currency = "eur" | "usd";
export type SubscriptionPlan = "pro" | "max5x" | "max20x" | "ultra" | "team";
export type Effort = "low" | "medium" | "high" | "xhigh" | "max";
export type TaskSize = "tiny" | "small" | "medium" | "large" | "huge";
export type Plan = "community" | "trial" | "pro";

export interface Caps {
  per_task_usd: number | null;
  per_day_usd: number | null;
  per_month_usd: number | null;
}

export interface Irreversible {
  allow_git_push: boolean;
  allow_deploy: boolean;
  allow_publish: boolean;
  allow_destructive_delete: boolean;
}

export type SkillPolicy = "strict" | "standard" | "open";

export interface Settings {
  language: string;
  ui_mode: UiMode;
  autonomy: Autonomy;
  connection_mode: ConnectionMode;
  currency: Currency;
  usd_to_eur: number;
  plan: SubscriptionPlan;
  caps: Caps;
  irreversible: Irreversible;
  skill_policy: SkillPolicy;
  screenshots: boolean;
  auto_retry: boolean;
  /** Express path for small precise requests in projects with tests. */
  express: boolean;
  onboarded: boolean;
}

// ---------- Phase 4: night pilot ----------

export interface NightOptions {
  budget_usd: number | null;
  stop_at: number | null;
  retry_failed: boolean;
  resume_margin_secs: number;
}

export type ItemOutcome = "done" | "failed" | "stopped" | "skipped" | "not_run";

export interface NightItem {
  task_id: string;
  title: string;
  outcome: ItemOutcome;
  verdict: Verdict | null;
  cost_usd: number;
  duration_ms: number;
  pauses: number;
  retried: boolean;
  note: string | null;
}

export interface NightPause {
  task_id: string;
  at: number;
  until: number;
  reason: string;
}

export interface NightReport {
  started_at: number;
  finished_at: number | null;
  options: NightOptions | null;
  items: NightItem[];
  pauses: NightPause[];
  total_cost_usd: number;
  cancelled: boolean;
}

export type NightEvent =
  | { kind: "started"; count: number }
  | { kind: "task_started"; task_id: string; title: string; index: number }
  | { kind: "task_finished"; item: NightItem }
  | { kind: "paused"; task_id: string; until: number; reason: string }
  | { kind: "resumed"; task_id: string }
  | { kind: "finished"; report: NightReport };

export interface NightView {
  queue: TaskRecord[];
  running: boolean;
  last: NightReport | null;
}

// ---------- Phase 4: freelance ----------

export interface FreelanceProfile {
  client_name: string;
  client_details: string;
  freelancer_name: string;
  hourly_rate: number;
  bill_ai_costs: boolean;
  markup_pct: number;
  review_minutes: number;
  footer: string;
}

export interface ClientQuote {
  currency: Currency;
  human_minutes: number;
  labour: number;
  ai_low: number;
  ai_likely: number;
  ai_high: number;
  total_low: number;
  total_likely: number;
  total_high: number;
}

export interface StatementLine {
  task_id: string;
  finished_at: number;
  title: string;
  agent_minutes: number;
  human_minutes: number;
  labour: number;
  ai_cost: number;
  ai_billed: number;
  total: number;
  verdict: Verdict | null;
  criteria_passed: number;
  criteria_total: number;
}

export interface Statement {
  project_name: string;
  profile: FreelanceProfile;
  currency: Currency;
  from: number;
  to: number;
  lines: StatementLine[];
  human_minutes: number;
  agent_minutes: number;
  labour: number;
  ai_cost: number;
  ai_billed: number;
  total: number;
}

// ---------- Phase 5: licence ----------

export type LicenceKind = "trial" | "pro" | "lifetime";

export interface LicenceStatus {
  plan: Plan;
  kind: LicenceKind | null;
  until: number | null;
  days_left: number | null;
  trial_refused: boolean;
  has_key: boolean;
}

export interface LicenceView {
  status: LicenceStatus;
  /** Before paid plans: every feature free, no licence network call. */
  early_access?: boolean;
  device: string;
  key_masked: string | null;
  api_base: string;
  site_url: string;
}

export interface RefreshRequest {
  key: string | null;
  device: string;
  token_exp: number | null;
}

export interface Shot {
  device: "desktop" | "mobile" | string;
  path: string;
  width: number;
  height: number;
}

// ---------- Phase 3 ----------

export interface FileChange {
  change: "added" | "modified" | "deleted" | "renamed";
  path: string;
}

export interface Leak {
  path: string;
  line: number;
  kind: "api_key" | "private_key" | "password" | "sensitive_file" | "iban" | "card_number";
  preview: string;
}

export interface Snapshot {
  id: string;
  at: number;
  /** "before:<task>:<title>", "after:<task>:<title>" or "restore:<id>:" */
  label: string;
}

export type Fix =
  | { kind: "install_claude" }
  | { kind: "update_claude" }
  | { kind: "sign_in" }
  | { kind: "add_api_key" }
  | { kind: "install_git" }
  | { kind: "open_url"; url: string }
  | { kind: "remove_mcp"; name: string };

export interface DoctorItem {
  id: string;
  name: string;
  state: "ok" | "warn" | "error";
  detail: string;
  fixes: Fix[];
}

export interface Finding {
  kind: string;
  severity: "low" | "medium" | "high";
  path: string;
  line: number;
  excerpt: string;
}

export interface ScanReport {
  name: string;
  description: string;
  score: number;
  verdict: "trusted" | "caution" | "dangerous";
  files: number;
  findings: Finding[];
  has_skill_md: boolean;
}

export interface ClaudeStatus {
  found: boolean;
  version: string | null;
  version_ok: boolean;
  subscription_signed_in: boolean;
  auth_method: string;
  plan: SubscriptionPlan | null;
}

export interface Spent {
  today_usd: number;
  month_usd: number;
}

export interface AppStatus {
  product: string;
  version: string;
  settings: Settings;
  claude: ClaudeStatus;
  api_key_set: boolean;
  plan: Plan;
  licence?: LicenceStatus | null;
  spent: Spent;
  subscription_mode_enabled: boolean;
  early_access?: boolean;
  /** "windows x86_64", for the feedback form. */
  os?: string;
  repo_url?: string;
}

export interface Project {
  id: string;
  path: string;
  name: string;
  created_at: number;
}

export interface ProjectStats {
  files: number;
  bytes: number;
  stack: string[];
  top_level: string[];
  is_git: boolean;
  has_claude_md: boolean;
  truncated: boolean;
}

export interface MemoryItem {
  key: string;
  question: string;
  answer: string;
}

export interface ProjectView {
  project: Project;
  stats: ProjectStats;
  memory: MemoryItem[];
}

export interface QuestionOption {
  label: string;
  description: string;
}

export interface Question {
  key: string;
  question: string;
  why: string;
  options: QuestionOption[];
  recommended: number;
}

export interface Answer {
  key: string;
  question: string;
  answer: string;
}

export type Check =
  | { kind: "command"; command: string }
  | { kind: "file_exists"; path: string }
  | { kind: "file_contains"; path: string; pattern: string }
  | { kind: "judge" }
  | { kind: "manual" };

export interface Criterion {
  id: string;
  description: string;
  check: Check;
}

export interface TaskSpec {
  title: string;
  role: string;
  context: string;
  objective: string;
  constraints: string[];
  deliverables: string[];
  success_criteria: Criterion[];
  output_format: string;
  size: TaskSize;
  risks: string[];
  direct?: boolean;
  /** Built without a model call and checked by the project's tests only. */
  express?: boolean;
}

export interface Route {
  /** "default" = Claude Code's own default model. */
  model: string;
  /** null = Claude Code's own default effort. */
  effort: Effort | null;
  permission_mode: string;
  max_turns: number;
  allowed_tools: string[];
  disallowed_tools: string[];
  plan_first: boolean;
}

export interface QuotaEstimate {
  low_pct: number;
  likely_pct: number;
  high_pct: number;
  measured: boolean;
}

export interface Quote {
  size: TaskSize;
  model: string;
  effort: Effort;
  low_usd: number;
  likely_usd: number;
  high_usd: number;
  overhead_usd: number;
  tokens_likely: number;
  calibrated: boolean;
  samples: number;
  quota: QuotaEstimate | null;
}

export type CapReason = "task_cap" | "day_cap" | "month_cap";

export type CapCheck =
  | { status: "ok"; effective_cap_usd: number | null }
  | { status: "warn"; reason: CapReason; effective_cap_usd: number }
  | { status: "blocked"; reason: CapReason; remaining_usd: number };

export interface TokenUsage {
  input_tokens: number;
  output_tokens: number;
  cache_creation_input_tokens: number;
  cache_read_input_tokens: number;
}

export interface PermissionDenial {
  tool_name: string;
  summary: string;
}

export interface Attempt {
  started_at: number;
  session_id: string | null;
  model: string | null;
  cost_usd: number;
  usage: TokenUsage;
  turns: number;
  duration_ms: number;
  summary: string;
  stopped: "budget" | "cancelled" | "retry_limit" | null;
  error: string | null;
  denials: PermissionDenial[];
  snapshot_before: string | null;
  snapshot_after: string | null;
  changes: FileChange[];
  leaks: Leak[];
  screenshots?: Shot[];
}

export type ItemStatus = "passed" | "failed" | "needs_approval" | "manual" | "error";
export type Verdict = "success" | "partial" | "failure" | "pending";

export interface ReportItem {
  id: string;
  description: string;
  kind: string;
  detail: string;
  status: ItemStatus;
  evidence: string;
}

export interface Report {
  items: ReportItem[];
  passed: number;
  failed: number;
  pending: number;
  verdict: Verdict;
  judge_cost_usd: number;
}

export type TaskStatus = "draft" | "planned" | "blocked" | "running" | "verifying" | "done" | "failed" | "stopped";

export interface Timings {
  prepare_ms: number;
  brief_ms: number;
  run_ms: number;
  screens_ms: number;
  verify_ms: number;
}

export interface TaskRecord {
  id: string;
  project_id: string;
  request: string;
  status: TaskStatus;
  mode: ConnectionMode;
  created_at: number;
  finished_at: number | null;
  questions: Question[];
  answers: Answer[];
  spec: TaskSpec | null;
  prompt: string | null;
  route: Route | null;
  quote: Quote | null;
  cap_check: CapCheck | null;
  attempts: Attempt[];
  report: Report | null;
  approved_commands: string[];
  manual: Record<string, boolean>;
  total_cost_usd: number;
  timings?: Timings;
  /** "question": read-only answer, no brief and no verification. */
  kind?: "task" | "question";
  /** The task this one continues (same Claude Code session). */
  parent_id?: string | null;
}

export type StreamEvent =
  | { kind: "init"; session_id: string; model: string }
  | { kind: "text"; text: string }
  | { kind: "tool_use"; id: string; name: string; summary: string }
  | { kind: "tool_result"; tool_use_id: string; is_error: boolean }
  | { kind: "usage"; message_id: string; model: string | null; usage: TokenUsage }
  | { kind: "api_error"; code: string; message: string }
  | { kind: "retry"; reason: string }
  | { kind: "rate_limit"; info: { status: string; limit_type: string | null; resets_at: number | null } }
  | { kind: "result"; result: { is_error: boolean; text: string; total_cost_usd: number } };

export interface LiveEvent {
  task_id: string;
  event: StreamEvent;
  cost_so_far_usd: number;
}

export interface RouteOverrides {
  model?: string | null;
  effort?: Effort | null;
  max_turns?: number | null;
}

export interface ApiError {
  code: string;
  message: string;
}
