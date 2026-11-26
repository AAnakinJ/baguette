// The app (Rust) verifies this exact token in `licence::tests::server_fixture_verifies`.
// Ed25519 signatures are deterministic, so the Worker must produce the same bytes.
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { verifyToken } from "../src/token.ts";
import { DEVICE, TEST_KEY, testSigner } from "./helpers.ts";

const FILE = fileURLToPath(new URL("./fixture-token.txt", import.meta.url));

describe("token format shared with the app", () => {
  it("signs the fixture exactly as stored", async () => {
    const sign = await testSigner();
    const token = await sign({ v: 1, typ: "licence", kind: "pro", lid: "lic_fixture", dev: DEVICE, iat: 1_700_000_000, exp: 1_900_000_000 });
    if (process.env.UPDATE_FIXTURE) writeFileSync(FILE, `${token}\n`);
    expect(token).toBe(readFileSync(FILE, "utf8").trim());
    expect(await verifyToken(TEST_KEY.publicRaw, token)).toMatchObject({ kind: "pro", dev: DEVICE });
  });

  it("refuses a modified token", async () => {
    const sign = await testSigner();
    const token = await sign({ v: 1, typ: "flags", iat: 1, exp: 2, subscription_mode: true });
    const [p, s] = token.split(".");
    const forged = `${Buffer.from(Buffer.from(p, "base64url").toString().replace("true", "false")).toString("base64url")}.${s}`;
    expect(await verifyToken(TEST_KEY.publicRaw, forged)).toBeNull();
  });
});
