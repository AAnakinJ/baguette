// Publishes the public part of this repository into publish/baguette, a separate git
// repository WITHOUT the private history, then commits there. Push from that folder.
//   node scripts/publish.mjs "Release 0.2.0"
// Safety: files listed in .publicignore are never copied, and the copy is scanned for
// secrets and personal data before anything is committed.
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { dirname, join, relative, sep } from "node:path";

const ROOT = process.cwd();
const TARGET = join(ROOT, "publish", "baguette");
const message = process.argv[2] ?? "Update";

const git = (cwd, ...args) => execFileSync("git", args, { cwd, encoding: "utf8" }).trim();

function patterns() {
  return readFileSync(join(ROOT, ".publicignore"), "utf8")
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l) => l && !l.startsWith("#"))
    .map((p) => {
      const folder = p.endsWith("/");
      const body = p.replace(/\/$/, "").replace(/[.+^${}()|[\]\\]/g, "\\$&").replace(/\*/g, "[^/]*");
      return new RegExp(`^${body}${folder ? "/" : "$"}`);
    });
}

/** Secrets or personal data that must never reach the public repository. */
const FORBIDDEN = [
  /\b(sk|rk)_(test|live)_[A-Za-z0-9]{10,}/,
  /\bwhsec_[A-Za-z0-9]{10,}/,
  /\bsk-ant-[A-Za-z0-9-]{20,}/,
  /-----BEGIN (RSA |EC )?PRIVATE KEY-----\n[A-Za-z0-9+/]{40}/,
  /\bFlowUP\b/i,
  /@gmail\.com/i,
];

export function publicFiles() {
  const ignore = patterns();
  return git(ROOT, "ls-files").split("\n").filter((f) => f && !ignore.some((re) => re.test(f)));
}

/** Obvious placeholders used by the tests (alphabet runs, "test", "example"). */
const FAKE = /abcdefghij|AbCdEfGhIj|test-0123456789|0123456789abcdef|very-secret-value|example|xxxxxxxx/i;

function scan(files) {
  const problems = [];
  for (const f of files) {
    const text = readFileSync(join(ROOT, f), "utf8");
    for (const re of FORBIDDEN) {
      for (const m of text.matchAll(new RegExp(re.source, `${re.flags.replace("g", "")}g`))) {
        if (!FAKE.test(m[0])) problems.push(`${f}: ${m[0].slice(0, 16)}…`);
      }
    }
  }
  return problems;
}

function main() {
  const files = publicFiles();
  const problems = scan(files.filter((f) => !/\.(png|ico|icns|jpg|gif)$/.test(f)));
  if (problems.length) {
    console.error("Refusing to publish, found:\n" + problems.join("\n"));
    process.exit(1);
  }
  if (!existsSync(join(TARGET, ".git"))) {
    mkdirSync(TARGET, { recursive: true });
    git(TARGET, "init", "-b", "main");
  }
  git(TARGET, "config", "user.name", process.env.PUBLISH_NAME ?? "AAnakinJ");
  git(TARGET, "config", "user.email", process.env.PUBLISH_EMAIL ?? "214685626+AAnakinJ@users.noreply.github.com");
  // Mirror: remove what is no longer public, then copy.
  const keep = new Set(files.map((f) => f.split("/").join(sep)));
  const walk = (dir) => {
    for (const e of readdirSync(dir)) {
      if (dir === TARGET && e === ".git") continue;
      const p = join(dir, e);
      if (statSync(p).isDirectory()) walk(p);
      else if (!keep.has(relative(TARGET, p))) rmSync(p);
    }
  };
  walk(TARGET);
  for (const f of files) {
    const to = join(TARGET, f);
    mkdirSync(dirname(to), { recursive: true });
    copyFileSync(join(ROOT, f), to);
  }
  git(TARGET, "add", "-A");
  if (!git(TARGET, "status", "--porcelain")) {
    console.log("Nothing new to publish.");
    return;
  }
  git(TARGET, "commit", "-q", "-m", message);
  console.log(`Published ${files.length} files to ${TARGET} (${git(TARGET, "log", "--oneline", "-1")}).`);
  console.log("Next: cd publish/baguette && git push origin main");
}

if (process.argv[1] && import.meta.url.endsWith(process.argv[1].split(sep).join("/").split("/").pop())) main();
