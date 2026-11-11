import type { TimeSyncResponse } from "../shared/api";
import { json, mediaBaseUrl, text } from "./common";
import { STATION_NAME } from "./station";

export { Station } from "./station";

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const { pathname } = new URL(request.url);
    const res = await route(request, env, pathname);
    return withCacheHeaders(pathname, res);
  },
} satisfies ExportedHandler<Env>;

async function route(
  request: Request,
  env: Env,
  pathname: string,
): Promise<Response> {
  if (request.method === "GET") {
    if (pathname === "/health") return text("OK");
    // Clock endpoints stay at the edge: they measure round trips, so they should be close by.
    if (pathname === "/api/utc") return utcTime();
    if (pathname === "/api/sync/time") return syncTime();
  }

  if (pathname.startsWith("/api/") || pathname.startsWith("/stream/")) {
    const station = env.STATION.get(env.STATION.idFromName(STATION_NAME));
    return station.fetch(request);
  }

  return serveAsset(request, env, pathname);
}

/** DASH UTCTiming wants a plain-text xs:dateTime body. */
function utcTime(): Response {
  return text(new Date().toISOString(), 200, { "Cache-Control": "no-store" });
}

function syncTime(): Response {
  const now = new Date();
  return json<TimeSyncResponse>(
    { timestamp: now.toISOString(), unix_millis: now.getTime() },
    { headers: { "Cache-Control": "no-store" } },
  );
}

async function serveAsset(
  request: Request,
  env: Env,
  pathname: string,
): Promise<Response> {
  const res = await env.ASSETS.fetch(request);
  if (res.status !== 404) return withMediaPreconnect(res, env);

  // Send unknown browser navigations to the dedicated 404 page; keep fetches as plain 404s.
  if (pathname === "/404")
    return Response.redirect(new URL("/404/", request.url).toString(), 307);
  if (pathname.startsWith("/404/")) return text("Not Found", 404);
  const accept = request.headers.get("Accept") ?? "";
  if (
    request.method === "GET" &&
    (accept.includes("text/html") || accept.includes("*/*"))
  ) {
    return Response.redirect(new URL("/404/", request.url).toString(), 307);
  }
  return text("Not Found", 404);
}

/**
 * Lets pages open the media origin's connection while the player is still loading, rather than
 * when it asks for its first segment. The player's segment requests are CORS fetches, which use
 * a separate connection from plain ones such as cover images, so both are warmed.
 */
function withMediaPreconnect(res: Response, env: Env): Response {
  if (!res.ok || !res.headers.get("Content-Type")?.startsWith("text/html"))
    return res;
  let origin: string;
  try {
    origin = new URL(mediaBaseUrl(env)).origin;
  } catch {
    return res; // A misconfigured media URL breaks playback, not the page.
  }
  const out = new Response(res.body, res);
  out.headers.append(
    "Link",
    `<${origin}>; rel=preconnect; crossorigin, <${origin}>; rel=preconnect`,
  );
  return out;
}

/** Cache-Control policy by route group, for successful responses only. */
function withCacheHeaders(pathname: string, res: Response): Response {
  if (!res.ok) return res;

  let value: string | undefined;
  let override = false;
  if (pathname === "/stream/manifest.mpd") {
    // Dynamic MPD: always revalidate.
    [value, override] = ["no-cache", true];
  } else if (
    pathname.startsWith("/api/utc") ||
    pathname.startsWith("/api/sync/time")
  ) {
    value = "no-store";
  } else if (pathname.startsWith("/api/admin/")) {
    // Authenticated + mutable state.
    [value, override] = ["no-store", true];
  } else if (
    pathname.startsWith("/api/tracks/") ||
    pathname.startsWith("/api/albums/")
  ) {
    // Metadata is stable enough to cache briefly.
    value = "public, max-age=300";
  } else if (pathname.startsWith("/assets/")) {
    // Vite emits content-hashed filenames here.
    [value, override] = ["public, max-age=31536000, immutable", true];
  }

  if (value === undefined || (!override && res.headers.has("Cache-Control")))
    return res;
  // Responses from other fetches carry immutable headers.
  const out = new Response(res.body, res);
  out.headers.set("Cache-Control", value);
  return out;
}
