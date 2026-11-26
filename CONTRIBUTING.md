# Contributing

Thanks for helping. Bug reports, ideas and translations are as welcome as code.

## Report a bug or suggest an idea

Open an issue with the matching template. For bugs, say which system, which Claude Code version (`claude --version`) and whether you use a subscription or an API key. **Never paste an API key, a token or private code** in an issue.

## Change the code

1. Fork, then create a branch.
2. Make the change with its tests. The engine is tested against a fake Claude Code (`crates/fake-claude`), so tests never spend tokens.
3. Check that everything passes:

```bash
npm ci && npm run typecheck && npm test
cargo test --workspace --exclude baguette-desktop
cargo build -p baguette-desktop
```

4. For changes that touch the interface and the engine together, also check the real app (no tokens spent): start it with a debug port, then let the script call every command from inside the window.

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9229"; npm run tauri dev
node scripts/e2e-app.mjs screenshot.png
```

5. Open a pull request that explains the why, not only the what.

## Ground rules

- Claude Code is driven only through its official CLI, never modified, and the user's sign-in is never read or relayed.
- Nothing about the user leaves their computer without a clear reason shown in the interface.
- Every interface string exists in the 10 languages (`src/locales`); the translation test enforces it.
- Irreversible actions stay blocked by default.

## Licence of contributions

By contributing, you agree that your contribution is published under the project licence ([FSL-1.1-ALv2](LICENSE.md)).
