// Signed tokens: base64url(payload JSON) "." base64url(Ed25519 signature over those bytes).
// The desktop app verifies them with the public key it ships with (crates/baguette-core/src/licence.rs).

const enc = new TextEncoder();
const dec = new TextDecoder();

export function b64url(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export function unb64url(s: string): Uint8Array<ArrayBuffer> {
  const bin = atob(s.replace(/-/g, "+").replace(/_/g, "/") + "===".slice((s.length + 3) % 4));
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
}

export type Signer = (payload: Record<string, unknown>) => Promise<string>;

/** `jwk` is the private key as a JWK JSON string ({kty:"OKP", crv:"Ed25519", d, x}). */
export async function signerFromJwk(jwk: string): Promise<Signer> {
  const key = await crypto.subtle.importKey("jwk", JSON.parse(jwk), { name: "Ed25519" }, false, ["sign"]);
  return async (payload) => {
    const bytes = enc.encode(JSON.stringify(payload));
    const sig = new Uint8Array(await crypto.subtle.sign({ name: "Ed25519" }, key, bytes));
    return `${b64url(bytes)}.${b64url(sig)}`;
  };
}

/** Returns the payload when the signature is valid (used by tests and tooling). */
export async function verifyToken(publicRaw: Uint8Array<ArrayBuffer>, token: string): Promise<Record<string, unknown> | null> {
  const [p, s, extra] = token.split(".");
  if (!p || !s || extra !== undefined) return null;
  const key = await crypto.subtle.importKey("raw", publicRaw, { name: "Ed25519" }, false, ["verify"]);
  const bytes = unb64url(p);
  const ok = await crypto.subtle.verify({ name: "Ed25519" }, key, unb64url(s), bytes);
  return ok ? JSON.parse(dec.decode(bytes)) : null;
}
