// Licence keys: "PRO-7KQ2-M9XD-4TJH-WC3P" (80 random bits, no 0/O/1/I).
// Stored as a SHA-256 hash for lookup, and encrypted (AES-GCM) so it can be shown again or
// resent by email; never in clear.
import { b64url, unb64url } from "./token.ts";

const ALPHABET = "23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
export const KEY_RE = /^(PRO|LIFE)(-[2-9A-HJ-NP-Z]{4}){4}$/;
export const DEVICE_RE = /^[0-9a-f]{32}$/;

export type LicenceKind = "pro" | "lifetime";

export function newLicenceKey(kind: LicenceKind): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  const chars = Array.from(bytes, (b) => ALPHABET[b & 31]).join("");
  return `${kind === "lifetime" ? "LIFE" : "PRO"}-${chars.match(/.{4}/g)!.join("-")}`;
}

export function normalizeKey(key: string): string {
  return key.trim().toUpperCase();
}

async function sha256Hex(text: string): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text)));
  return Array.from(digest, (b) => b.toString(16).padStart(2, "0")).join("");
}

export const hashKey = (key: string) => sha256Hex(`key:${normalizeKey(key)}`);
/** Lets a buyer get their keys back by email without storing the address itself. */
export const hashEmail = (email: string) => sha256Hex(`email:${email.trim().toLowerCase()}`);

async function aesKey(secretB64: string): Promise<CryptoKey> {
  const raw = unb64url(secretB64);
  if (raw.length !== 32) throw new Error("KEY_ENC_SECRET must be 32 bytes (base64url)");
  return crypto.subtle.importKey("raw", raw, { name: "AES-GCM" }, false, ["encrypt", "decrypt"]);
}

export async function encryptKey(key: string, secretB64: string): Promise<string> {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = new Uint8Array(await crypto.subtle.encrypt({ name: "AES-GCM", iv }, await aesKey(secretB64), new TextEncoder().encode(key)));
  return `${b64url(iv)}.${b64url(ct)}`;
}

export async function decryptKey(sealed: string, secretB64: string): Promise<string> {
  const [iv, ct] = sealed.split(".");
  const pt = await crypto.subtle.decrypt({ name: "AES-GCM", iv: unb64url(iv) }, await aesKey(secretB64), unb64url(ct));
  return new TextDecoder().decode(pt);
}
