// Local licence server for end-to-end tests with Stripe TEST mode, on http://localhost:8787.
//   STRIPE_SECRET_KEY=sk_test_... (or rk_test_...) STRIPE_WEBHOOK_SECRET=whsec_... node server/scripts/dev.mjs
// Uses a throwaway signing key (printed public key) and a SQLite file in the temp folder:
// nothing here is production data.
import { createServer } from "node:http";
import { generateKeyPairSync, randomBytes } from "node:crypto";
import { readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createHandler } from "../src/index.ts";
import { D1Store } from "../src/store.ts";
import { stripeApi } from "../src/stripe.ts";
import { signerFromJwk } from "../src/token.ts";
import { nodeD1 } from "./node-d1.mjs";

const key = process.env.STRIPE_SECRET_KEY ?? "";
if (!/^(sk|rk)_test_/.test(key)) {
  console.error("Test mode only: set STRIPE_SECRET_KEY=sk_test_... or rk_test_...");
  process.exit(1);
}
const port = Number(process.env.PORT ?? 8787);
const { privateKey } = generateKeyPairSync("ed25519");
const jwk = privateKey.export({ format: "jwk" });
const dbFile = join(tmpdir(), "baguette-licence-dev.sqlite");

const env = {
  DB: null,
  SIGNING_KEY: JSON.stringify(jwk),
  KEY_ENC_SECRET: randomBytes(32).toString("base64url"),
  STRIPE_SECRET_KEY: key,
  STRIPE_WEBHOOK_SECRET: process.env.STRIPE_WEBHOOK_SECRET ?? "",
  STRIPE_PRICES: readFileSync(new URL("../stripe-prices.test.json", import.meta.url), "utf8"),
  SITE_URL: process.env.SITE_URL ?? "http://localhost:5174",
  API_BASE: `http://localhost:${port}`,
  PRODUCT_NAME: "Baguette",
};

const handler = createHandler({
  env,
  store: new D1Store(nodeD1(dbFile)),
  sign: await signerFromJwk(env.SIGNING_KEY),
  stripe: stripeApi(key),
  now: () => Math.floor(Date.now() / 1000),
});

createServer(async (req, res) => {
  const chunks = [];
  for await (const c of req) chunks.push(c);
  const body = chunks.length ? Buffer.concat(chunks) : undefined;
  const request = new Request(`http://localhost:${port}${req.url}`, {
    method: req.method,
    headers: Object.entries(req.headers).flatMap(([k, v]) => (Array.isArray(v) ? v.map((x) => [k, x]) : v === undefined ? [] : [[k, v]])),
    body: req.method === "GET" || req.method === "HEAD" ? undefined : body,
  });
  const response = await handler(request);
  const text = Buffer.from(await response.arrayBuffer());
  res.writeHead(response.status, Object.fromEntries(response.headers));
  res.end(text);
  const url = new URL(request.url);
  console.log(`${new Date().toISOString().slice(11, 19)} ${req.method} ${url.pathname} -> ${response.status}${url.pathname.endsWith("webhook") ? ` ${req.headers["stripe-signature"] ? "(signed)" : ""}` : ""}`);
}).listen(port, () => {
  console.log(`licence server (TEST) on http://localhost:${port}`);
  console.log(`public key: ${jwk.x}`);
  console.log(`database: ${dbFile}`);
});
