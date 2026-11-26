// Signed in-app updates (Tauri updater): only a release signed with the project's update key
// can be installed. Outside the desktop app (browser preview), "?fakeupdate" simulates one.
import { inTauri } from "./api";

export interface UpdateInfo {
  version: string;
  notes: string;
  /** Downloads, installs and restarts the app. `onProgress` gets 0-100, or null if unknown. */
  install: (onProgress: (pct: number | null) => void) => Promise<void>;
}

export async function checkUpdate(): Promise<UpdateInfo | null> {
  if (!inTauri) {
    if (typeof location === "undefined" || !location.search.includes("fakeupdate")) return null;
    return {
      version: "0.2.0",
      notes: "",
      install: async (onProgress) => {
        for (const p of [10, 45, 80, 100]) {
          await new Promise((r) => setTimeout(r, 300));
          onProgress(p);
        }
      },
    };
  }
  const { check } = await import("@tauri-apps/plugin-updater");
  const update = await check();
  if (!update) return null;
  return {
    version: update.version,
    notes: update.body ?? "",
    install: async (onProgress) => {
      let total = 0;
      let done = 0;
      await update.downloadAndInstall((e) => {
        if (e.event === "Started") total = e.data.contentLength ?? 0;
        else if (e.event === "Progress") {
          done += e.data.chunkLength;
          onProgress(total ? Math.min(100, Math.round((done * 100) / total)) : null);
        }
      });
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    },
  };
}
