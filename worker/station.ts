import { DurableObject } from "cloudflare:workers";

import type {
  AdminAlbumSummaryResponse,
  AdminLoginRequest,
  AdminOkResponse,
  AlbumResponse,
  NowResponse,
  ReloadSummary,
  SchedulingState,
  SetAlbumEnabledRequest,
  TrackInfoResponse,
} from "../shared/api";
import {
  ADMIN_SESSION_COOKIE,
  buildClearCookie,
  buildSetCookie,
  newSessionToken,
  parseCookie,
  verifyAdminPassword,
} from "./auth";
import type { Album } from "./catalogue";
import { Catalogue, encodedDurationSeconds, loadAlbums } from "./catalogue";
import { HttpError, json, mediaBaseUrl, streamingConfig, text } from "./common";
import { generateMpd } from "./dash";
import type { ScheduleState, SchedulingSnapshot } from "./scheduler";
import { Scheduler } from "./scheduler";

const SCHEDULE_KEY = "schedule";
const DISABLED_KEY = "disabled";
const SESSION_PREFIX = "session:";
/** One stored album per R2 metadata key, so storage lists them in playlist order. */
const CATALOGUE_PREFIX = "catalogue:";
/** Storage reads and writes take at most this many keys per call. */
const STORAGE_BATCH = 128;

/** Name of the one station every request is routed to. */
export const STATION_NAME = "station";

function joinUrl(base: string, path: string): string {
  return `${base.replace(/\/+$/, "")}/${path.replace(/^\/+/, "")}`;
}

function schedulingStateFor(
  albumId: string,
  snap: SchedulingSnapshot,
): SchedulingState | undefined {
  if (snap.nowPlayingAlbumId === albumId) return "playing";
  if (snap.queuedAlbumIds.has(albumId)) return "queued";
  return undefined;
}

async function readJson<T>(request: Request): Promise<T> {
  try {
    return await request.json<T>();
  } catch (e) {
    throw new HttpError(400, `Invalid JSON body: ${(e as Error).message}`);
  }
}

/**
 * The radio station: the schedule every listener shares, the admin's album toggles and the admin
 * sessions, all persisted in this object's storage so an eviction resumes rather than resets.
 *
 * The catalogue is read from R2 and kept in memory, with a parsed copy in storage: waking up from
 * that copy skips listing, fetching and parsing every metadata file before the first answer. A
 * wake-up then re-reads R2 in the background, as does `POST /api/admin/reload`.
 */
export class Station extends DurableObject<Env> {
  private scheduler!: Scheduler;
  private syncing = false;
  private rateWindow = { startMs: 0, count: 0 };

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    void ctx.blockConcurrencyWhile(async () => {
      const [stored, state, disabled] = await Promise.all([
        ctx.storage.list<Album>({ prefix: CATALOGUE_PREFIX }),
        ctx.storage.get<ScheduleState>(SCHEDULE_KEY),
        ctx.storage.get<string[]>(DISABLED_KEY),
      ]);
      let catalogue: Catalogue;
      if (stored.size > 0) {
        catalogue = new Catalogue([...stored.values()]);
      } else {
        // First start: there is nothing to serve until the bucket has been read.
        const albums = await loadAlbums(env.MEDIA);
        await this.storeCatalogue(albums, stored);
        catalogue = new Catalogue([...albums.values()]);
      }
      const disabledSet = new Set(disabled ?? []);
      if (state) {
        this.scheduler = new Scheduler(state, catalogue, disabledSet);
      } else {
        this.scheduler = Scheduler.create(
          catalogue,
          disabledSet,
          Date.now(),
          env.SUGGESTED_PRESENTATION_DELAY,
        );
        await ctx.storage.put(SCHEDULE_KEY, this.scheduler.state);
      }
      console.log(
        `Loaded ${catalogue.albums.length} albums with ${catalogue.totalTracks} total tracks`,
      );

      // The stored copy may predate an upload; catch up without holding up requests.
      if (stored.size > 0) {
        void this.syncCatalogue().catch((e) =>
          console.warn(
            `background catalogue refresh failed (serving stored copy): ${(e as Error).message}`,
          ),
        );
      }
    });
  }

  async fetch(request: Request): Promise<Response> {
    try {
      const nowMs = Date.now();
      await this.maintain(nowMs);
      return await this.route(request, nowMs);
    } catch (e) {
      if (e instanceof HttpError) return text(e.message, e.status);
      console.error(e);
      return text((e as Error).message, 500);
    }
  }

  /** Slide the schedule to now. Best effort: a failure still lets us serve the existing window. */
  private async maintain(nowMs: number): Promise<void> {
    try {
      if (this.scheduler.maintain(nowMs, streamingConfig(this.env))) {
        await this.ctx.storage.put(SCHEDULE_KEY, this.scheduler.state);
      }
    } catch (e) {
      console.warn(
        `scheduler maintain failed (serving existing window): ${(e as Error).message}`,
      );
    }
  }

  private async route(request: Request, nowMs: number): Promise<Response> {
    const { pathname, searchParams } = new URL(request.url);
    const method = request.method;

    if (pathname === "/stream/manifest.mpd" && method === "GET")
      return this.manifest(nowMs);
    if (pathname === "/api/now" && method === "GET")
      return this.now(searchParams, nowMs);

    let m = pathname.match(/^\/api\/tracks\/([^/]+)$/);
    if (m && method === "GET") return this.trackInfo(decodeURIComponent(m[1]));
    m = pathname.match(/^\/api\/albums\/([^/]+)$/);
    if (m && method === "GET") return this.album(decodeURIComponent(m[1]));

    if (pathname.startsWith("/api/admin/"))
      return this.admin(request, pathname.slice("/api/admin".length), nowMs);

    return text("Not Found", 404);
  }

  private manifest(nowMs: number): Response {
    const config = streamingConfig(this.env);
    const periods = this.scheduler
      .periods(nowMs, config)
      .map(({ track, scheduled }) => ({
        track,
        startSeconds: scheduled.startSeconds,
        durationSeconds: scheduled.durationSeconds,
      }));
    const mpd = generateMpd(
      { ...config, mediaBaseUrl: mediaBaseUrl(this.env) },
      this.scheduler.state.epochMs,
      nowMs,
      periods,
    );
    return new Response(mpd, {
      headers: {
        "Content-Type": "application/dash+xml",
        // Dynamic MPD: avoid stale manifests through proxies.
        "Cache-Control": "no-cache",
      },
    });
  }

  /**
   * What is on air at `t` on the presentation timeline, defaulting to the live edge.
   *
   * A browser that refuses autoplay leaves the player with no moving playhead and a manifest it
   * fetched once, so the schedule itself is the only reliable source.
   */
  private now(params: URLSearchParams, nowMs: number): Response {
    const raw = params.get("t");
    const seconds =
      raw === null ? this.scheduler.secondsAt(nowMs) : Number(raw);
    if (raw !== null && (raw.trim() === "" || !Number.isFinite(seconds))) {
      throw new HttpError(400, `Invalid t: ${raw}`);
    }
    const scheduled = this.scheduler.scheduledAt(seconds);
    if (!scheduled)
      throw new HttpError(404, `nothing scheduled at t=${seconds}`);
    return json<NowResponse>({
      track_id: scheduled.trackId,
      album_id: scheduled.albumId,
      period_start_seconds: scheduled.startSeconds,
      duration_seconds: scheduled.durationSeconds,
    });
  }

  private trackInfo(trackId: string): Response {
    const found = this.scheduler.catalogue.getTrack(trackId);
    if (!found) throw new HttpError(404, "Track not found");
    const { album, track } = found;
    return json<TrackInfoResponse>({
      album_id: album.id,
      album_name: album.name,
      album_artist: album.artist,
      cover_url: joinUrl(mediaBaseUrl(this.env), album.cover_path),
      track_id: track.id,
      track_title: track.title,
      track_artist: track.artist ?? null,
      track_number: track.track_number,
      disc_number: track.disc_number,
      duration_seconds: encodedDurationSeconds(track),
    });
  }

  private album(albumId: string): Response {
    const album = this.scheduler.catalogue.getAlbum(albumId);
    if (!album) throw new HttpError(404, "Album not found");
    return json<AlbumResponse>({
      album_id: album.id,
      album_name: album.name,
      album_artist: album.artist,
      release_date: album.release_date ?? null,
      original_release_date: album.original_release_date ?? null,
      album_label: album.label ?? null,
      album_media: album.media ?? null,
      album_catalog_number: album.catalog_number ?? null,
      musicbrainz_album_id: album.musicbrainz_album_id ?? null,
      disc_count: album.disc_count,
      cover_url: joinUrl(mediaBaseUrl(this.env), album.cover_path),
      tracks: album.tracks.map((track) => ({
        track_id: track.id,
        track_title: track.title,
        track_artist: track.artist ?? null,
        track_number: track.track_number,
        disc_number: track.disc_number,
        duration_seconds: track.length_seconds,
      })),
    });
  }

  // ---- admin ----

  private async admin(
    request: Request,
    path: string,
    nowMs: number,
  ): Promise<Response> {
    this.takeAdminRateLimit(nowMs);
    const method = request.method;

    if (path === "/login" && method === "POST")
      return this.login(request, nowMs);
    if (path === "/logout" && method === "POST") return this.logout(request);

    await this.requireSession(request, nowMs);

    if (path === "/me" && method === "GET")
      return json<AdminOkResponse>({ ok: true });
    if (path === "/albums" && method === "GET") return this.adminAlbums(nowMs);
    const m = path.match(/^\/albums\/([^/]+)\/enabled$/);
    if (m && method === "PUT")
      return this.setAlbumEnabled(request, decodeURIComponent(m[1]), nowMs);
    if (path === "/reload" && method === "POST") return this.reload();

    return text("Not Found", 404);
  }

  /** A fixed one-second window across all admin endpoints; it guards password verification. */
  private takeAdminRateLimit(nowMs: number): void {
    const limit = Math.max(1, this.env.ADMIN_RATE_LIMIT_PER_SECOND);
    if (nowMs - this.rateWindow.startMs >= 1000)
      this.rateWindow = { startMs: nowMs, count: 0 };
    if (this.rateWindow.count >= limit)
      throw new HttpError(429, "Rate limit exceeded");
    this.rateWindow.count += 1;
  }

  private sessionTtlSeconds(): number {
    return Math.max(60, this.env.ADMIN_SESSION_TTL_SECONDS);
  }

  private async requireSession(request: Request, nowMs: number): Promise<void> {
    const token = parseCookie(request.headers, ADMIN_SESSION_COOKIE);
    if (!token) throw new HttpError(401, "Not logged in");
    const key = SESSION_PREFIX + token;
    const expiresAt = await this.ctx.storage.get<number>(key);
    if (expiresAt === undefined) throw new HttpError(401, "Not logged in");
    if (expiresAt <= nowMs) {
      await this.ctx.storage.delete(key);
      throw new HttpError(401, "Not logged in");
    }
  }

  private async login(request: Request, nowMs: number): Promise<Response> {
    const req = await readJson<Partial<AdminLoginRequest>>(request);
    if (typeof req.username !== "string" || typeof req.password !== "string") {
      throw new HttpError(400, "Expected {username, password}");
    }
    const creds = {
      username: this.env.SCRUTINIZER_USERNAME,
      passwordHash: this.env.SCRUTINIZER_PASSWORD_HASH,
    };
    if (!verifyAdminPassword(creds, req.username, req.password)) {
      throw new HttpError(401, "Invalid credentials");
    }

    await this.dropExpiredSessions(nowMs);
    const token = newSessionToken();
    const ttl = this.sessionTtlSeconds();
    await this.ctx.storage.put(SESSION_PREFIX + token, nowMs + ttl * 1000);

    const secure = new URL(request.url).protocol === "https:";
    return json<AdminOkResponse>(
      { ok: true },
      { headers: { "Set-Cookie": buildSetCookie(token, ttl, secure) } },
    );
  }

  private async logout(request: Request): Promise<Response> {
    const token = parseCookie(request.headers, ADMIN_SESSION_COOKIE);
    if (token) await this.ctx.storage.delete(SESSION_PREFIX + token);
    const secure = new URL(request.url).protocol === "https:";
    return json<AdminOkResponse>(
      { ok: true },
      { headers: { "Set-Cookie": buildClearCookie(secure) } },
    );
  }

  /** Sessions that are never presented again would otherwise stay in storage forever. */
  private async dropExpiredSessions(nowMs: number): Promise<void> {
    const sessions = await this.ctx.storage.list<number>({
      prefix: SESSION_PREFIX,
    });
    const expired = [...sessions]
      .filter(([, expiresAt]) => expiresAt <= nowMs)
      .map(([key]) => key);
    if (expired.length > 0) await this.ctx.storage.delete(expired);
  }

  private albumSummary(
    album: Album,
    snap: SchedulingSnapshot,
  ): AdminAlbumSummaryResponse {
    const summary: AdminAlbumSummaryResponse = {
      album_id: album.id,
      album_name: album.name,
      album_artist: album.artist,
      enabled: !this.scheduler.disabled.has(album.id),
      track_count: album.tracks.length,
      cover_url: joinUrl(mediaBaseUrl(this.env), album.cover_path),
    };
    const state = schedulingStateFor(album.id, snap);
    if (state) summary.scheduling_state = state;
    return summary;
  }

  private adminAlbums(nowMs: number): Response {
    const snap = this.scheduler.snapshot(nowMs);
    const albums = this.scheduler.catalogue.albums.map((a) =>
      this.albumSummary(a, snap),
    );
    albums.sort(
      (a, b) =>
        a.album_artist.localeCompare(b.album_artist) ||
        a.album_name.localeCompare(b.album_name),
    );
    return json(albums);
  }

  private async setAlbumEnabled(
    request: Request,
    albumId: string,
    nowMs: number,
  ): Promise<Response> {
    const req = await readJson<Partial<SetAlbumEnabledRequest>>(request);
    if (typeof req.enabled !== "boolean")
      throw new HttpError(400, "Expected {enabled: boolean}");

    const album = this.scheduler.catalogue.getAlbum(albumId);
    if (!album) throw new HttpError(404, "Album not found");

    const disabled = new Set(this.scheduler.disabled);
    if (req.enabled) disabled.delete(albumId);
    else disabled.add(albumId);
    this.scheduler.disabled = disabled;
    await this.ctx.storage.put(DISABLED_KEY, [...disabled]);

    return json(this.albumSummary(album, this.scheduler.snapshot(nowMs)));
  }

  /**
   * Re-read the catalogue from R2 and swap it in. On any error (e.g. malformed YAML) the current
   * catalogue keeps serving. The schedule is left alone, so listeners' timeline stays continuous;
   * the next `maintain` prunes whatever the new catalogue dropped.
   */
  private async reload(): Promise<Response> {
    const summary = await this.syncCatalogue();
    console.log("metadata reloaded", summary);
    return json(summary);
  }

  /**
   * Re-reads the catalogue from R2, adopts it and stores it. A failed read keeps the current one.
   * Only one runs at a time: each re-reads every metadata object.
   */
  private async syncCatalogue(): Promise<ReloadSummary> {
    if (this.syncing) throw new HttpError(409, "reload already in progress");
    this.syncing = true;
    try {
      let albums: Map<string, Album>;
      try {
        albums = await loadAlbums(this.env.MEDIA);
      } catch (e) {
        throw new HttpError(500, `reload failed: ${(e as Error).message}`);
      }
      const catalogue = new Catalogue([...albums.values()]);

      const before = new Set(this.scheduler.catalogue.albums.map((a) => a.id));
      const after = new Set(catalogue.albums.map((a) => a.id));
      // Toggles for albums that no longer exist are dropped; new albums start enabled.
      const disabled = new Set(
        [...this.scheduler.disabled].filter((id) => after.has(id)),
      );

      this.scheduler.catalogue = catalogue;
      this.scheduler.disabled = disabled;
      await Promise.all([
        this.ctx.storage.put(DISABLED_KEY, [...disabled]),
        this.storeCatalogue(albums),
      ]);

      return {
        albums_before: before.size,
        albums_after: after.size,
        added: [...after].filter((id) => !before.has(id)).length,
        removed: [...before].filter((id) => !after.has(id)).length,
        kept_disabled: disabled.size,
      };
    } finally {
      this.syncing = false;
    }
  }

  /**
   * Mirrors `albums` (keyed by R2 key) into storage, writing only the albums that changed.
   * `stored` is what storage holds now, if the caller has already listed it.
   */
  private async storeCatalogue(
    albums: Map<string, Album>,
    stored?: Map<string, Album>,
  ): Promise<void> {
    const current =
      stored ??
      (await this.ctx.storage.list<Album>({ prefix: CATALOGUE_PREFIX }));
    const puts = [...albums]
      .map(([key, album]) => [CATALOGUE_PREFIX + key, album] as const)
      .filter(
        ([key, album]) =>
          JSON.stringify(current.get(key)) !== JSON.stringify(album),
      );
    const deletes = [...current.keys()].filter(
      (key) => !albums.has(key.slice(CATALOGUE_PREFIX.length)),
    );
    for (let i = 0; i < puts.length; i += STORAGE_BATCH) {
      await this.ctx.storage.put(
        Object.fromEntries(puts.slice(i, i + STORAGE_BATCH)),
      );
    }
    for (let i = 0; i < deletes.length; i += STORAGE_BATCH) {
      await this.ctx.storage.delete(deletes.slice(i, i + STORAGE_BATCH));
    }
  }
}
