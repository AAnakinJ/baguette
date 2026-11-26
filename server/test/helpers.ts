import { createPrivateKey } from "node:crypto";
import { signerFromJwk, unb64url } from "../src/token.ts";

/** Ed25519 key from a fixed 32-byte seed (the Rust tests use the same seed: [7; 32]). */
export function jwkFromSeed(seed: number): { jwk: string; publicRaw: Uint8Array } {
  const der = Buffer.concat([Buffer.from("302e020100300506032b657004220420", "hex"), Buffer.alloc(32, seed)]);
  const jwk = createPrivateKey({ key: der, format: "der", type: "pkcs8" }).export({ format: "jwk" }) as { x: string };
  return { jwk: JSON.stringify(jwk), publicRaw: unb64url(jwk.x) };
}

export const TEST_KEY = jwkFromSeed(7);
export const testSigner = () => signerFromJwk(TEST_KEY.jwk);
/** 32 bytes of 1, base64url: the AES key for stored licence keys in tests. */
export const ENC_SECRET = Buffer.alloc(32, 1).toString("base64url");
export const DEVICE = "0123456789abcdef0123456789abcdef";
export const DEVICE2 = "fedcba9876543210fedcba9876543210";
