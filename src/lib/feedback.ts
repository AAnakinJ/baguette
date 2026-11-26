// "Give feedback": opens the GitHub issue form with the technical details already filled in.
// Only the app version, the system, the Claude Code version and the interface language go into
// the link. Nothing is sent by the app: the user reads the form and submits it on GitHub.
import type { AppStatus } from "./types";

const FALLBACK_REPO = "https://github.com/AAnakinJ/baguette";

export function feedbackUrl(status: AppStatus): string {
  const repo = (status.repo_url ?? FALLBACK_REPO).replace(/\/+$/, "");
  const q = new URLSearchParams({
    template: "feedback.yml",
    app: status.version,
    system: status.os ?? "",
    claude: status.claude.version ?? "",
    language: status.settings.language,
    mode: status.settings.ui_mode,
  });
  return `${repo}/issues/new?${q.toString()}`;
}
