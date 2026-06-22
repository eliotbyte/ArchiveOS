export type MiniPlayerCorner =
  | "bottom-left"
  | "bottom-right"
  | "top-left"
  | "top-right";

const DRAG_THRESHOLD_PX = 5;

export function snapMiniCorner(
  centerX: number,
  centerY: number,
  viewportWidth: number,
  viewportHeight: number,
): MiniPlayerCorner {
  const horizontal = centerX < viewportWidth / 2 ? "left" : "right";
  const vertical = centerY < viewportHeight / 2 ? "top" : "bottom";
  return `${vertical}-${horizontal}` as MiniPlayerCorner;
}

export function pointerMovedBeyondThreshold(
  startX: number,
  startY: number,
  currentX: number,
  currentY: number,
  threshold = DRAG_THRESHOLD_PX,
): boolean {
  const dx = currentX - startX;
  const dy = currentY - startY;
  return Math.hypot(dx, dy) > threshold;
}

export { DRAG_THRESHOLD_PX };
