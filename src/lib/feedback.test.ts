import { describe, expect, it } from "vitest";
import { feedbackUrl } from "./feedback";
import type { AppStatus } from "./types";

const status = {
  product: "Baguette",
  version: "0.1.1",
  os: "windows x86_64",
  repo_url: "https://github.com/AAnakinJ/baguette/",
  claude: { found: true, version: "2.1.220", version_ok: true, subscription_signed_in: true, auth_method: "claude.ai", plan: "pro" },
  settings: { language: "fr", ui_mode: "beginner" },
} as unknown as AppStatus;

describe("feedback link", () => {
  it("opens the GitHub feedback form with the technical details filled in", () => {
    const url = new URL(feedbackUrl(status));
    expect(url.origin + url.pathname).toBe("https://github.com/AAnakinJ/baguette/issues/new");
    expect(Object.fromEntries(url.searchParams)).toEqual({
      template: "feedback.yml",
      app: "0.1.1",
      system: "windows x86_64",
      claude: "2.1.220",
      language: "fr",
      mode: "beginner",
    });
  });

  it("carries nothing else: no project, no path, no account", () => {
    const url = feedbackUrl({ ...status, api_key_set: true, spent: { today_usd: 1, month_usd: 2 } } as AppStatus);
    expect(url).not.toMatch(/Users|path|key|spent|@/i);
  });
});
