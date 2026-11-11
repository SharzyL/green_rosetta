import { abortAllDurableObjects } from "cloudflare:test";
import { env, exports } from "cloudflare:workers";
import { beforeAll, describe, expect, it, vi } from "vitest";

import type {
  AdminAlbumSummaryResponse,
  AlbumResponse,
  NowResponse,
  ReloadSummary,
  TimeSyncResponse,
  TrackInfoResponse,
} from "../../shared/api";
import { albumYaml } from "./fixtures";

const BASE = "https://radio.test";

function get(path: string, init?: RequestInit): Promise<Response> {
  return exports.default.fetch(new Request(BASE + path, init));
}

function send(
  method: string,
  path: string,
  body: unknown,
  cookie?: string,
): Promise<Response> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (cookie) headers.Cookie = cookie;
  return get(path, { method, headers, body: JSON.stringify(body) });
}

async function login(): Promise<string> {
  const res = await send("POST", "/api/admin/login", {
    username: "admin",
    password: env.TEST_ADMIN_PASSWORD,
  });
  expect(res.status).toBe(200);
  const setCookie = res.headers.get("Set-Cookie") ?? "";
  expect(setCookie).toMatch(
    /^gr_admin_session=[\w-]{43}; HttpOnly; SameSite=Lax; Path=\/api\/admin; Max-Age=86400; Secure$/,
  );
  return setCookie.split(";")[0];
}

beforeAll(async () => {
  // 40 tracks of 12s per album: far longer than the manifest window.
  await env.MEDIA.put("metadata/a.yaml", albumYaml("A", [40]));
  await env.MEDIA.put("metadata/b.yaml", albumYaml("B", [20, 20]));
  await env.MEDIA.put("metadata/readme.txt", "not metadata");
});

describe("streaming", () => {
  it("serves a dynamic manifest pointing at the public media origin", async () => {
    const res = await get("/stream/manifest.mpd");
    expect(res.status).toBe(200);
    expect(res.headers.get("Content-Type")).toBe("application/dash+xml");
    expect(res.headers.get("Cache-Control")).toBe("no-cache");
    const mpd = await res.text();
    expect(mpd).toContain('type="dynamic"');
    expect(mpd).toMatch(
      /<Period id="period_[AB]_d\dt\d+" start="PT\d+\.\d{3}S"/,
    );
    expect(mpd).toContain(
      'initialization="https://syborg.sharzy.in/media/albums/',
    );
  });

  it("keeps the timeline when the station is evicted", async () => {
    const epoch = (mpd: string) =>
      mpd.match(/availabilityStartTime="([^"]+)"/)?.[1];
    const before = epoch(await (await get("/stream/manifest.mpd")).text());
    await abortAllDurableObjects();
    const after = epoch(await (await get("/stream/manifest.mpd")).text());
    expect(before).toMatch(/Z$/);
    expect(after).toBe(before);
  });

  it("says what is on air, and serves its track and album", async () => {
    const now = await get("/api/now");
    expect(now.status).toBe(200);
    const onAir = await now.json<NowResponse>();
    expect(onAir.duration_seconds).toBe(12);

    const track = await get(`/api/tracks/${onAir.track_id}`);
    expect(track.headers.get("Cache-Control")).toBe("public, max-age=300");
    const info = await track.json<TrackInfoResponse>();
    expect(info).toMatchObject({
      track_id: onAir.track_id,
      album_id: onAir.album_id,
      duration_seconds: 12,
    });
    expect(info.cover_url).toMatch(
      /^https:\/\/syborg\.sharzy\.in\/media\/albums\/.+\/cover\.jpg$/,
    );

    const album = await (
      await get(`/api/albums/${onAir.album_id}`)
    ).json<AlbumResponse>();
    expect(album.album_id).toBe(onAir.album_id);
    expect(album.tracks).toHaveLength(40);
    expect(album.tracks[0]).toMatchObject({
      track_artist: "Artist",
      duration_seconds: 11.5,
    });
    expect(album.release_date).toBe("1962");
  });

  it("answers for a given playhead and rejects bad ones", async () => {
    const { period_start_seconds, track_id } = await (
      await get("/api/now")
    ).json<NowResponse>();
    const res = await get(`/api/now?t=${period_start_seconds + 0.5}`);
    expect((await res.json<NowResponse>()).track_id).toBe(track_id);

    expect((await get("/api/now?t=abc")).status).toBe(400);
    expect((await get("/api/now?t=-100")).status).toBe(404);
  });

  it("404s unknown tracks and albums", async () => {
    expect((await get("/api/tracks/nope")).status).toBe(404);
    expect((await get("/api/albums/nope")).status).toBe(404);
    expect((await get("/api/nope")).status).toBe(404);
  });
});

describe("time", () => {
  it("serves UTC for DASH UTCTiming", async () => {
    const res = await get("/api/utc");
    expect(res.headers.get("Cache-Control")).toBe("no-store");
    expect(await res.text()).toMatch(
      /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/,
    );
  });

  it("serves the sync helper", async () => {
    const res = await get("/api/sync/time");
    const body = await res.json<TimeSyncResponse>();
    expect(Date.parse(body.timestamp)).toBe(body.unix_millis);
  });
});

describe("admin", () => {
  it("requires a session", async () => {
    expect((await get("/api/admin/me")).status).toBe(401);
    expect(
      (
        await get("/api/admin/albums", {
          headers: { Cookie: "gr_admin_session=forged" },
        })
      ).status,
    ).toBe(401);
  });

  it("rejects wrong credentials", async () => {
    expect(
      (
        await send("POST", "/api/admin/login", {
          username: "admin",
          password: "nope",
        })
      ).status,
    ).toBe(401);
    expect(
      (
        await send("POST", "/api/admin/login", {
          username: "root",
          password: env.TEST_ADMIN_PASSWORD,
        })
      ).status,
    ).toBe(401);
    expect(
      (await send("POST", "/api/admin/login", { username: "admin" })).status,
    ).toBe(400);
  });

  it("logs in and out", async () => {
    const cookie = await login();
    const me = await get("/api/admin/me", { headers: { Cookie: cookie } });
    expect(me.status).toBe(200);
    expect(me.headers.get("Cache-Control")).toBe("no-store");

    const out = await send("POST", "/api/admin/logout", {}, cookie);
    expect(out.headers.get("Set-Cookie")).toContain("Max-Age=0");
    expect(
      (await get("/api/admin/me", { headers: { Cookie: cookie } })).status,
    ).toBe(401);
  });

  it("lists albums, toggles one and keeps the toggle across a reload", async () => {
    const cookie = await login();
    const albums = await (
      await get("/api/admin/albums", { headers: { Cookie: cookie } })
    ).json<AdminAlbumSummaryResponse[]>();
    expect(albums.map((a) => a.album_id)).toEqual(["A", "B"]);
    expect(albums.some((a) => a.scheduling_state === "playing")).toBe(true);

    const toggled = await send(
      "PUT",
      "/api/admin/albums/B/enabled",
      { enabled: false },
      cookie,
    );
    expect(await toggled.json<AdminAlbumSummaryResponse>()).toMatchObject({
      album_id: "B",
      enabled: false,
      track_count: 40,
    });
    expect(
      (
        await send(
          "PUT",
          "/api/admin/albums/Z/enabled",
          { enabled: false },
          cookie,
        )
      ).status,
    ).toBe(404);

    await env.MEDIA.put("metadata/c.yaml", albumYaml("C", [3]));
    const reload = await send("POST", "/api/admin/reload", {}, cookie);
    expect(await reload.json<ReloadSummary>()).toEqual({
      albums_before: 2,
      albums_after: 3,
      added: 1,
      removed: 0,
      kept_disabled: 1,
    });

    await abortAllDurableObjects();
    const after = await (
      await get("/api/admin/albums", { headers: { Cookie: cookie } })
    ).json<AdminAlbumSummaryResponse[]>();
    expect(after.map((a) => [a.album_id, a.enabled])).toEqual([
      ["A", true],
      ["B", false],
      ["C", true],
    ]);
  });

  it("wakes up from its stored catalogue and catches up with the bucket", async () => {
    // Metadata that no longer parses must not keep the station from waking up.
    await env.MEDIA.put("metadata/broken.yaml", "id: [");
    await abortAllDurableObjects();
    expect((await get("/api/albums/A")).status).toBe(200);
    await env.MEDIA.delete("metadata/broken.yaml");

    await env.MEDIA.put("metadata/d.yaml", albumYaml("D", [2]));
    await abortAllDurableObjects();
    await vi.waitFor(async () =>
      expect((await get("/api/albums/D")).status).toBe(200),
    );
  });

  it("keeps serving the old catalogue when a reload fails", async () => {
    const cookie = await login();
    await env.MEDIA.put("metadata/broken.yaml", "id: [");
    const res = await send("POST", "/api/admin/reload", {}, cookie);
    expect(res.status).toBe(500);
    expect(await res.text()).toMatch(
      /^reload failed: .*metadata\/broken\.yaml/,
    );
    await env.MEDIA.delete("metadata/broken.yaml");
    expect((await get("/api/albums/A")).status).toBe(200);
  });
});

describe("static", () => {
  it("answers health checks", async () => {
    expect(await (await get("/health")).text()).toBe("OK");
  });

  it("sends unknown pages to the 404 page and keeps fetches plain", async () => {
    const page = await get("/no/such/page", {
      headers: { Accept: "text/html" },
      redirect: "manual",
    });
    expect(page.status).toBe(307);
    expect(page.headers.get("Location")).toBe(`${BASE}/404/`);

    const fetchRes = await get("/no/such/file.js", {
      headers: { Accept: "application/json" },
    });
    expect(fetchRes.status).toBe(404);
  });
});
