type HalftoneOptions = {
  stepPx: number;
  rMinPx: number;
  rMaxPx: number;
  dotColor: string;
  opacityTop: number;
  opacityBottom: number;
  invert: boolean; // when true: large at top, small at bottom
  angleDeg: number;
};

function clamp01(n: number): number {
  return Math.min(1, Math.max(0, n));
}

function parseCssNumber(v: string): number | null {
  const n = Number.parseFloat(v.trim());
  return Number.isFinite(n) ? n : null;
}

function readOptionsFromCss(): HalftoneOptions {
  const s = getComputedStyle(document.documentElement);
  const stepPx = parseCssNumber(s.getPropertyValue("--halftone-step-px")) ?? 26;
  const rMinPx =
    parseCssNumber(s.getPropertyValue("--halftone-r-min-px")) ?? 1.25;
  const rMaxPx =
    parseCssNumber(s.getPropertyValue("--halftone-r-max-px")) ?? 8.5;
  const opacityTop =
    parseCssNumber(s.getPropertyValue("--halftone-opacity-top")) ?? 0.55;
  const opacityBottom =
    parseCssNumber(s.getPropertyValue("--halftone-opacity-bottom")) ?? 0.08;
  const invert =
    (s.getPropertyValue("--halftone-invert") || "").trim().toLowerCase() ===
    "true";
  const angleDeg =
    parseCssNumber(s.getPropertyValue("--halftone-angle-deg")) ?? 30;
  const dotColor =
    s.getPropertyValue("--halftone-dot-color").trim() || "rgba(90, 0, 55, 1)";

  return {
    stepPx: Math.max(10, stepPx),
    rMinPx: Math.max(0.25, rMinPx),
    rMaxPx: Math.max(0.5, rMaxPx),
    dotColor,
    opacityTop: clamp01(opacityTop),
    opacityBottom: clamp01(opacityBottom),
    invert,
    angleDeg,
  };
}

function ensureCanvas(): HTMLCanvasElement {
  const existing = document.getElementById("halftone-bg");
  if (existing instanceof HTMLCanvasElement) return existing;

  const canvas = document.createElement("canvas");
  canvas.id = "halftone-bg";
  canvas.setAttribute("aria-hidden", "true");
  // Style is in index.css; keep this minimal.
  document.body.append(canvas);
  return canvas;
}

function draw(canvas: HTMLCanvasElement): void {
  const options = readOptionsFromCss();

  const rect = canvas.getBoundingClientRect();
  const cssW = Math.max(1, Math.floor(rect.width));
  const cssH = Math.max(1, Math.floor(rect.height));
  const dpr = Math.max(1, Math.min(window.devicePixelRatio || 1, 3));

  canvas.width = Math.floor(cssW * dpr);
  canvas.height = Math.floor(cssH * dpr);

  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, cssW, cssH);

  ctx.fillStyle = options.dotColor;

  const step = options.stepPx;

  // Slant the dot grid by rotating the dot *positions* by ~30deg, while keeping dot size based
  // only on screen-space y (so size depends only on y).
  const angle = (options.angleDeg * Math.PI) / 180;
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  const cx0 = cssW / 2;
  const cy0 = cssH / 2;

  // Overscan so the rotated grid covers the full viewport.
  const diag = Math.sqrt(cssW * cssW + cssH * cssH);
  const extra = step * 2;
  const startX = cx0 - diag / 2 - extra;
  const endX = cx0 + diag / 2 + extra;
  const startY = cy0 - diag / 2 - extra;
  const endY = cy0 + diag / 2 + extra;

  for (let gy = startY; gy <= endY; gy += step) {
    for (let gx = startX; gx <= endX; gx += step) {
      const dx = gx - cx0;
      const dy = gy - cy0;
      const rx = cx0 + dx * cos - dy * sin;
      const ry = cy0 + dx * sin + dy * cos;

      const yNorm = clamp01(ry / Math.max(1, cssH));
      const t = options.invert ? 1 - yNorm : yNorm;
      const eased = t ** 1.8;

      // Radius and alpha ramp (based only on y).
      const rUnclamped =
        options.rMinPx + (options.rMaxPx - options.rMinPx) * eased;
      const r = rUnclamped;
      const alpha =
        options.opacityTop +
        (options.opacityBottom - options.opacityTop) * eased;

      ctx.globalAlpha = alpha;
      ctx.beginPath();
      ctx.arc(rx, ry, r, 0, Math.PI * 2);
      ctx.fill();
    }
  }
}

export function mountHalftoneBackground(): void {
  if (typeof window === "undefined") return;

  const canvas = ensureCanvas();

  let raf = 0;
  const schedule = () => {
    if (raf) cancelAnimationFrame(raf);
    raf = requestAnimationFrame(() => draw(canvas));
  };

  const resize = () => schedule();
  window.addEventListener("resize", resize, { passive: true });

  // Ensure we draw once after layout stabilizes.
  schedule();
}
