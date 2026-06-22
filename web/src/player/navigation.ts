export function shouldDeferEntityNavigation(): boolean {
  return document.fullscreenElement !== null;
}
