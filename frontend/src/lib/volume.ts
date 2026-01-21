export const VOLUME_STORAGE_KEY = "green_rosetta_volume_v1";
export const MUTED_STORAGE_KEY = "green_rosetta_muted_v1";

export const clamp01 = (n: number): number => Math.min(1, Math.max(0, n));

export const loadVolume = (fallback = 0.8): number => {
  try {
    const stored = localStorage.getItem(VOLUME_STORAGE_KEY);
    const parsed = stored ? Number(stored) : NaN;
    if (Number.isFinite(parsed)) return clamp01(parsed);
  } catch {
    // ignore
  }
  return fallback;
};

export const saveVolume = (value: number): void => {
  try {
    localStorage.setItem(VOLUME_STORAGE_KEY, String(clamp01(value)));
  } catch {
    // ignore
  }
};

export const loadMuted = (): boolean => {
  try {
    return localStorage.getItem(MUTED_STORAGE_KEY) === "1";
  } catch {
    return false;
  }
};

export const saveMuted = (muted: boolean): void => {
  try {
    localStorage.setItem(MUTED_STORAGE_KEY, muted ? "1" : "0");
  } catch {
    // ignore
  }
};
