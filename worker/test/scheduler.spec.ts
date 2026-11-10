import { describe, expect, it } from "vitest";

import { Catalogue } from "../catalogue";
import type { ScheduledTrack } from "../scheduler";
import { endSeconds, Scheduler } from "../scheduler";
import { catalogue } from "./fixtures";

const WINDOW = { timeShiftBufferDepth: 60, minFutureManifestDuration: 60 };
const T0 = Date.UTC(2026, 0, 1);
const at = (seconds: number) => T0 + seconds * 1000;

/** A scheduler started at T0 (no presentation delay) on the first enabled album. */
function start(cat: Catalogue, disabled: string[] = []): Scheduler {
  return Scheduler.create(cat, new Set(disabled), T0, 0, () => 0);
}

const ids = (queue: ScheduledTrack[]) => queue.map((st) => st.trackId);

/** The window must be gapless: every track starts where the previous one ends. */
function expectContiguous(queue: ScheduledTrack[]) {
  for (let i = 1; i < queue.length; i++) {
    expect(queue[i].startSeconds).toBeCloseTo(endSeconds(queue[i - 1]));
  }
}

describe("Scheduler", () => {
  it("fills the window and plays albums in order, wrapping around", () => {
    // 12s tracks: A has 2, B has 1. The window reaches 60s ahead of now.
    const s = start(catalogue({ A: 2, B: 1 }));
    s.maintain(at(0), WINDOW);
    expect(ids(s.state.queue)).toEqual([
      "A_d1t1",
      "A_d1t2",
      "B_d1t1",
      "A_d1t1",
      "A_d1t2",
    ]);
    expectContiguous(s.state.queue);
  });

  it("wraps a single album onto itself", () => {
    const s = start(catalogue({ A: 2 }));
    s.maintain(at(0), WINDOW);
    expect(ids(s.state.queue).slice(0, 4)).toEqual([
      "A_d1t1",
      "A_d1t2",
      "A_d1t1",
      "A_d1t2",
    ]);
  });

  it("drops tracks once they leave the time-shift window", () => {
    const s = start(catalogue({ A: 50 }));
    s.maintain(at(0), WINDOW);
    s.maintain(at(200), WINDOW);
    const first = s.state.queue[0];
    expect(endSeconds(first)).toBeGreaterThanOrEqual(
      200 - WINDOW.timeShiftBufferDepth,
    );
    expect(first.startSeconds).toBeLessThanOrEqual(
      200 - WINDOW.timeShiftBufferDepth,
    );
    expect(
      endSeconds(s.state.queue[s.state.queue.length - 1]),
    ).toBeGreaterThanOrEqual(260);
    expectContiguous(s.state.queue);
  });

  it("reports whether the state changed", () => {
    const s = start(catalogue({ A: 50 }));
    expect(s.maintain(at(0), WINDOW)).toBe(true);
    expect(s.maintain(at(0), WINDOW)).toBe(false);
  });

  // Regression: a quiet spell lets the whole window age out. The station must carry on as if it
  // had played to an empty room, not restart at the first album.
  it("plays through an idle gap instead of restarting", () => {
    const s = start(catalogue({ A: 400 }));
    s.maintain(at(0), WINDOW);

    s.maintain(at(3600), WINDOW);
    const live = s.scheduledAt(3600);
    // 3600s / 12s per track: the 301st track is on air.
    expect(live?.trackId).toBe("A_d1t301");
    expect(live?.startSeconds).toBe(3600);
    expectContiguous(s.state.queue);
  });

  it("carries on across albums and wraps during an idle gap", () => {
    const s = start(catalogue({ A: 3, B: 2 }));
    s.maintain(at(0), WINDOW);
    // A cycle is 5 tracks = 60s, so t=6000 is 100 whole cycles in: back at A's first track.
    s.maintain(at(6000 + 1), WINDOW);
    expect(s.scheduledAt(6001)?.trackId).toBe("A_d1t1");
  });

  // Regression: deleting the album on air (so the whole window is that album) must not leave an
  // empty manifest; the scheduler prunes it and refills from the remaining albums.
  it("recovers when the album on air is deleted", () => {
    const cat = catalogue({ A: 20, B: 3, C: 3 });
    const s = start(cat);
    s.maintain(at(30), WINDOW);
    expect(s.snapshot(at(30)).nowPlayingAlbumId).toBe("A");

    s.catalogue = new Catalogue(cat.albums.filter((a) => a.id !== "A"));
    s.maintain(at(31), WINDOW);

    expect(s.periods(at(31), WINDOW).length).toBeGreaterThan(0);
    const playing = s.snapshot(at(31)).nowPlayingAlbumId;
    expect(playing).not.toBeNull();
    expect(playing).not.toBe("A");
  });

  it("skips disabled albums and picks a random one when the current album is disabled", () => {
    const s = start(catalogue({ A: 1, B: 1, C: 1 }), ["B"]);
    s.maintain(at(0), WINDOW);
    expect(ids(s.state.queue).slice(0, 4)).toEqual([
      "A_d1t1",
      "C_d1t1",
      "A_d1t1",
      "C_d1t1",
    ]);

    s.disabled = new Set(["A", "B"]);
    expect(s.nextInPlaylist("A", "A_d1t1").album.id).toBe("C");
  });

  it("fails to extend the window when no album is enabled", () => {
    const s = start(catalogue({ A: 2 }));
    s.maintain(at(0), WINDOW);
    s.disabled = new Set(["A"]);
    expect(() => s.maintain(at(100), WINDOW)).toThrow("No enabled albums");
  });

  it("answers what is on air and what is queued", () => {
    const s = start(catalogue({ A: 1, B: 1, C: 1 }));
    s.maintain(at(0), WINDOW);
    expect(s.scheduledAt(13)).toMatchObject({
      albumId: "B",
      startSeconds: 12,
      durationSeconds: 12,
    });
    expect(s.scheduledAt(-1)).toBeUndefined();

    const snap = s.snapshot(at(13));
    expect(snap.nowPlayingAlbumId).toBe("B");
    expect([...snap.queuedAlbumIds].sort()).toEqual(["A", "B", "C"]);
  });

  it("shifts the epoch back by the presentation delay", () => {
    const s = Scheduler.create(catalogue({ A: 1 }), new Set(), T0, 30, () => 0);
    expect(s.state.epochMs).toBe(T0 - 30_000);
    expect(s.secondsAt(T0)).toBe(30);
  });
});
