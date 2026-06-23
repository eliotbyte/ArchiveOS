/** Google Material Symbols names for smart / user list covers. */
export function libraryListMaterialIcon(listType: string): string | null {
  switch (listType) {
    case "smart_continue_watching":
      return "play_circle";
    case "smart_recently_added":
      return "hourglass_arrow_down";
    case "smart_recently_watched":
      return "history";
    case "watch_later":
      return "schedule";
    default:
      return null;
  }
}

export function libraryListShowsOverlay(listType: string, overlay?: boolean): boolean {
  if (overlay) return true;
  return libraryListMaterialIcon(listType) !== null;
}
