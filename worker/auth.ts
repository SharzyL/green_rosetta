import { compareSync } from "bcrypt-ts";

export const ADMIN_SESSION_COOKIE = "gr_admin_session";

/** Credentials as configured through secrets; either may be missing on a fresh deployment. */
export interface AdminCredentials {
  username?: string;
  passwordHash?: string;
}

export function verifyAdminPassword(
  creds: AdminCredentials,
  username: string,
  password: string,
): boolean {
  const expectedUser = creds.username?.trim() ?? "";
  const hash = creds.passwordHash?.trim() ?? "";
  // Unconfigured credentials lock the admin API rather than open it.
  if (expectedUser === "" || hash === "" || username !== expectedUser)
    return false;
  return compareSync(password, hash);
}

export function parseCookie(
  headers: Headers,
  name: string,
): string | undefined {
  const cookie = headers.get("Cookie");
  if (!cookie) return undefined;
  for (const part of cookie.split(";")) {
    const eq = part.indexOf("=");
    if (eq === -1) continue;
    if (part.slice(0, eq).trim() === name) return part.slice(eq + 1).trim();
  }
  return undefined;
}

/** 32 random bytes, base64url without padding. */
export function newSessionToken(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  return btoa(String.fromCharCode(...bytes))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/, "");
}

function cookieAttributes(maxAgeSeconds: number, secure: boolean): string[] {
  // - HttpOnly: JS cannot read it
  // - SameSite=Lax: avoids most CSRF by default
  // - Path=/api/admin: scope to admin API only
  // - Secure: only on HTTPS (local dev is plain HTTP)
  const parts = [
    "HttpOnly",
    "SameSite=Lax",
    "Path=/api/admin",
    `Max-Age=${maxAgeSeconds}`,
  ];
  if (secure) parts.push("Secure");
  return parts;
}

export function buildSetCookie(
  token: string,
  ttlSeconds: number,
  secure: boolean,
): string {
  return [
    `${ADMIN_SESSION_COOKIE}=${token}`,
    ...cookieAttributes(ttlSeconds, secure),
  ].join("; ");
}

export function buildClearCookie(secure: boolean): string {
  return [`${ADMIN_SESSION_COOKIE}=`, ...cookieAttributes(0, secure)].join(
    "; ",
  );
}
