/** An error that maps to an HTTP status, with the message as a plain-text body. */
export class HttpError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

export function text(
  body: string,
  status = 200,
  headers?: HeadersInit,
): Response {
  const res = new Response(body, { status, headers });
  res.headers.set("Content-Type", "text/plain; charset=utf-8");
  return res;
}

export function json<T>(body: T, init?: ResponseInit): Response {
  return Response.json(body, init);
}

export function streamingConfig(env: Env) {
  return {
    timeShiftBufferDepth: env.TIME_SHIFT_BUFFER_DEPTH,
    suggestedPresentationDelay: env.SUGGESTED_PRESENTATION_DELAY,
    minFutureManifestDuration: env.MIN_FUTURE_MANIFEST_DURATION,
  };
}

/** `{PUBLIC_MEDIA_URL}/media`, accepting a bare domain for the public URL. */
export function mediaBaseUrl(env: Env): string {
  const domain = env.PUBLIC_MEDIA_URL.trim().replace(/\/+$/, "");
  if (domain === "") throw new HttpError(500, "PUBLIC_MEDIA_URL must be set");
  const origin = /^https?:\/\//.test(domain) ? domain : `https://${domain}`;
  return `${origin}/media`;
}
