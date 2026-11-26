<p align="center"><img src="assets/logo.svg" width="96" alt=""></p>

<h1 align="center">Baguette</h1>
<p align="center"><b>The autopilot for Claude Code: the price before, the proof after — in your language.</b></p>
<p align="center"><a href="README.fr.md">Français</a> · <a href="docs/benchmark.md">Benchmark</a> · <a href="CONTRIBUTING.md">Contributing</a></p>

---

Baguette is a desktop app (Windows, Linux) that drives the official Claude Code for you. Describe what you want in your own words; Baguette asks the few questions that matter, shows the cost **before** anything runs, lets Claude Code do the work within your cap, then **proves** the result: tests, files, screenshots on a computer and a phone, and an independent reviewer.

**Early access: every feature is free.** No account, no card, nothing sent to us.

## What it does

| | |
|---|---|
| **Grill** | Only the questions that change the result, never twice in the same project. |
| **Brief** | Your words become a precise brief with checkable success criteria. |
| **Quote and cap** | Likely cost in money (API key) or in % of your usage window (subscription). The task stops at your cap. |
| **Run** | The official, unmodified `claude` CLI with the right model, effort and guard-rails. Irreversible actions (push, deploy, mass delete) are blocked unless you allow them. |
| **Proof** | Every criterion checked: commands, files, screenshots (desktop and phone), a read-only reviewer. Failed checks can be retried in the same session. |
| **Night pilot** | Queue prepared tasks for the night. When your quota runs out it waits for the reset and continues. Morning report. |
| **Freelance mode** | Client price on every quote, monthly activity statement with the proof of each task. |
| **Safety** | Timeline with one-click undo, leak guard for keys and personal data, skill antivirus, connector doctor. |
| **Share** | An animated GIF of the task: request, quote, result on computer and phone, proof. |

Interface in 10 languages: English, Español, Português (Brasil), Français, Deutsch, 日本語, 한국어, हिन्दी, العربية, Bahasa Indonesia. Beginner and Expert modes.

## How it connects to Claude

- **Your Claude subscription**: you sign in through Claude Code's own `claude auth login`. Baguette never reads, stores or relays your credentials; it runs the unmodified Claude Code binary, as Anthropic's [legal and compliance](https://code.claude.com/docs/en/legal-and-compliance) page describes.
- **An API key**: stored in the operating system's keychain and handed only to the Claude Code process.

Your code and your requests stay between you and Anthropic: Baguette runs on your computer and has no server of its own during early access.

Baguette is an independent project, **not affiliated with or endorsed by Anthropic**. Claude and Claude Code are trademarks of Anthropic, PBC.

## Install

Download the installer for your system from the [latest release](../../releases/latest). The app updates itself (signed updates).

Windows may show a SmartScreen warning because the installer is not code-signed yet: click **More info → Run anyway**.

## Development

```bash
npm ci
npm test                                            # interface, licence server and website tests
cargo test --workspace --exclude baguette-desktop   # engine + end-to-end tests against a fake Claude Code (no tokens spent)
npm run tauri dev                                   # run the desktop app
```

| Path | What |
|---|---|
| `crates/baguette-core` | The engine: Claude Code adapter, grill, brief, routing, quote, caps, verification, night pilot, freelance, timeline, local database |
| `crates/fake-claude` | A test double of the Claude Code CLI that replays scripted sessions |
| `src-tauri` | The desktop shell (Tauri 2) |
| `src` | The interface (React + TypeScript), translations in `src/locales` |
| `server` | Licence server for the future paid plan (Cloudflare Worker), unused during early access |
| `site` | The website generator (10 languages, no tracking) |
| `bench` | A/B benchmark: Claude Code alone vs Baguette on real tasks |

## Licence

[Functional Source License 1.1, Apache 2.0 future licence (FSL-1.1-ALv2)](LICENSE.md): you can read, use, modify and redistribute the code, including at work, except to offer a competing product. Each version becomes Apache-2.0 two years after its release.
