import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useLocation, useNavigate } from "react-router-dom";
import type {
  CollectionMemberItem,
  EntityAsset,
  EntityDetail,
  EntityListItem,
} from "../api/client";
import {
  buildQueueFromMembers,
  currentQueueItem,
  queueItemFromDetail,
  queueItemFromEntity,
  shuffleIndices,
  type PlayerQueueState,
  type QueueItem,
} from "../player/queue";
import { shouldDeferEntityNavigation } from "../player/navigation";
import type { MiniPlayerCorner } from "../player/miniPlayerSnap";

export type PlayerDisplayMode = "hidden" | "full" | "mini";

const MINI_CORNER_STORAGE_KEY = "archiveos-mini-corner";

const VALID_MINI_CORNERS: MiniPlayerCorner[] = [
  "bottom-left",
  "bottom-right",
  "top-left",
  "top-right",
];

function readStoredMiniCorner(): MiniPlayerCorner {
  try {
    const stored = localStorage.getItem(MINI_CORNER_STORAGE_KEY);
    if (stored === "left") return "bottom-left";
    if (stored === "right") return "bottom-right";
    if (VALID_MINI_CORNERS.includes(stored as MiniPlayerCorner)) {
      return stored as MiniPlayerCorner;
    }
  } catch {
    // ignore storage errors
  }
  return "bottom-right";
}

interface VideoPlayerContextValue {
  mode: PlayerDisplayMode;
  queue: PlayerQueueState;
  currentItem: QueueItem | null;
  playing: boolean;
  setPlaying: (playing: boolean) => void;
  playEntity: (entity: EntityListItem | EntityDetail) => void;
  playQueue: (
    members: CollectionMemberItem[],
    startEntityId?: string,
    options?: { shuffle?: boolean },
  ) => void;
  playNext: () => void;
  playPrevious: () => void;
  toggleShuffle: () => void;
  openFull: () => void;
  collapseToMini: () => void;
  closePlayer: () => void;
  seekVersion: number;
  notifySeek: () => void;
  savePlaybackPosition: (assetId: string, time: number) => void;
  getPlaybackPosition: (assetId: string) => number;
  fullPlayerAnchor: HTMLDivElement | null;
  registerFullPlayerAnchor: (element: HTMLDivElement | null) => void;
  miniPlayerDock: HTMLDivElement | null;
  registerMiniPlayerDock: (element: HTMLDivElement | null) => void;
  miniCorner: MiniPlayerCorner;
  setMiniCorner: (corner: MiniPlayerCorner) => void;
  miniDragOffset: { x: number; y: number };
  setMiniDragOffset: (offset: { x: number; y: number }) => void;
  activeEntityAssets: EntityAsset[];
  setActiveEntityAssets: (assets: EntityAsset[]) => void;
}

const emptyQueue: PlayerQueueState = {
  items: [],
  currentIndex: 0,
  shuffled: false,
  shuffleOrder: [],
};

const VideoPlayerContext = createContext<VideoPlayerContextValue | null>(null);

export function VideoPlayerProvider({ children }: { children: ReactNode }) {
  const location = useLocation();
  const navigate = useNavigate();
  const returnRouteRef = useRef("/library/youtube");
  const [mode, setMode] = useState<PlayerDisplayMode>("hidden");
  const modeRef = useRef<PlayerDisplayMode>("hidden");
  const [queue, setQueue] = useState<PlayerQueueState>(emptyQueue);
  const queueRef = useRef<PlayerQueueState>(emptyQueue);
  const locationRef = useRef(location);
  const [playing, setPlaying] = useState(false);
  const [seekVersion, setSeekVersion] = useState(0);
  const [fullPlayerAnchor, setFullPlayerAnchor] = useState<HTMLDivElement | null>(
    null,
  );
  const [miniPlayerDock, setMiniPlayerDock] = useState<HTMLDivElement | null>(null);
  const [miniCorner, setMiniCornerState] = useState<MiniPlayerCorner>(readStoredMiniCorner);
  const [miniDragOffset, setMiniDragOffset] = useState({ x: 0, y: 0 });
  const [activeEntityAssets, setActiveEntityAssets] = useState<EntityAsset[]>([]);
  const playbackPositionsRef = useRef<Map<string, number>>(new Map());
  const clearedPlaybackAssetsRef = useRef<Set<string>>(new Set());

  const savePlaybackPosition = useCallback((assetId: string, time: number) => {
    if (clearedPlaybackAssetsRef.current.has(assetId)) return;
    if (time > 0) {
      playbackPositionsRef.current.set(assetId, time);
    }
  }, []);

  const getPlaybackPosition = useCallback((assetId: string) => {
    return playbackPositionsRef.current.get(assetId) ?? 0;
  }, []);

  const currentItem = useMemo(() => currentQueueItem(queue), [queue]);

  useEffect(() => {
    queueRef.current = queue;
  }, [queue]);

  useEffect(() => {
    locationRef.current = location;
  }, [location]);

  useEffect(() => {
    modeRef.current = mode;
  }, [mode]);

  useEffect(() => {
    function handleFullscreenChange() {
      if (document.fullscreenElement) return;
      const item = currentQueueItem(queueRef.current);
      if (!item) return;
      const targetPath = `/entities/${item.entityId}`;
      if (locationRef.current.pathname !== targetPath) {
        navigate(targetPath);
      }
    }
    document.addEventListener("fullscreenchange", handleFullscreenChange);
    return () =>
      document.removeEventListener("fullscreenchange", handleFullscreenChange);
  }, [navigate]);

  const registerFullPlayerAnchor = useCallback((element: HTMLDivElement | null) => {
    setFullPlayerAnchor(element);
  }, []);

  const registerMiniPlayerDock = useCallback((element: HTMLDivElement | null) => {
    setMiniPlayerDock(element);
  }, []);

  const setMiniCorner = useCallback((corner: MiniPlayerCorner) => {
    setMiniCornerState(corner);
    try {
      localStorage.setItem(MINI_CORNER_STORAGE_KEY, corner);
    } catch {
      // ignore storage errors
    }
  }, []);

  useEffect(() => {
    if (!location.pathname.startsWith("/entities/")) {
      returnRouteRef.current = `${location.pathname}${location.search}`;
    }
  }, [location.pathname, location.search]);

  useEffect(() => {
    if (!currentItem) {
      setMode("hidden");
      return;
    }
    if (location.pathname.startsWith("/entities/")) {
      setMode("full");
      return;
    }
    if (playing || modeRef.current === "mini") {
      setMode("mini");
      return;
    }
    setQueue((current) => {
      const item = currentQueueItem(current);
      if (item) {
        clearedPlaybackAssetsRef.current.add(item.primaryAssetId);
        playbackPositionsRef.current.delete(item.primaryAssetId);
      }
      return emptyQueue;
    });
    setPlaying(false);
    setMode("hidden");
    setActiveEntityAssets([]);
  }, [location.pathname, currentItem, playing]);

  const playQueue = useCallback(
    (
      members: CollectionMemberItem[],
      startEntityId?: string,
      options?: { shuffle?: boolean },
    ) => {
      const items = buildQueueFromMembers(members);
      if (items.length === 0) return;

      const shuffled = options?.shuffle ?? false;
      const shuffleOrder = shuffled ? shuffleIndices(items.length) : [];
      const startIndex = startEntityId
        ? Math.max(
            0,
            items.findIndex((item) => item.entityId === startEntityId),
          )
        : 0;
      const currentIndex = shuffled
        ? shuffleOrder.findIndex((index) => index === startIndex)
        : startIndex;

      setQueue({
        items,
        currentIndex: currentIndex >= 0 ? currentIndex : 0,
        shuffled,
        shuffleOrder,
      });
      const target = items[startIndex >= 0 ? startIndex : 0];
      clearedPlaybackAssetsRef.current.delete(target.primaryAssetId);
      setPlaying(true);
      setMode("full");
      navigate(`/entities/${target.entityId}`);
    },
    [navigate],
  );

  const playEntity = useCallback(
    (entity: EntityListItem | EntityDetail) => {
      const item =
        "assets" in entity
          ? queueItemFromDetail(entity)
          : queueItemFromEntity(entity);
      if (!item) return;
      if ("assets" in entity) {
        setActiveEntityAssets(entity.assets);
      }
      setQueue({
        items: [item],
        currentIndex: 0,
        shuffled: false,
        shuffleOrder: [],
      });
      clearedPlaybackAssetsRef.current.delete(item.primaryAssetId);
      setPlaying(true);
      setMode("full");
      const targetPath = `/entities/${item.entityId}`;
      if (location.pathname !== targetPath) {
        navigate(targetPath);
      }
    },
    [navigate, location.pathname],
  );

  const playNext = useCallback(() => {
    setQueue((current) => {
      if (current.items.length <= 1) return current;
      const nextIndex = (current.currentIndex + 1) % current.items.length;
      const resolved = current.shuffled
        ? current.shuffleOrder[nextIndex]
        : nextIndex;
      const nextItem = current.items[resolved];
      if (nextItem && !shouldDeferEntityNavigation() && modeRef.current !== "mini") {
        navigate(`/entities/${nextItem.entityId}`);
      }
      return { ...current, currentIndex: nextIndex };
    });
    setPlaying(true);
  }, [navigate]);

  const playPrevious = useCallback(() => {
    setQueue((current) => {
      if (current.items.length <= 1) return current;
      const nextIndex =
        (current.currentIndex - 1 + current.items.length) % current.items.length;
      const resolved = current.shuffled
        ? current.shuffleOrder[nextIndex]
        : nextIndex;
      const nextItem = current.items[resolved];
      if (nextItem && !shouldDeferEntityNavigation() && modeRef.current !== "mini") {
        navigate(`/entities/${nextItem.entityId}`);
      }
      return { ...current, currentIndex: nextIndex };
    });
    setPlaying(true);
  }, [navigate]);

  const toggleShuffle = useCallback(() => {
    setQueue((current) => {
      if (current.shuffled) {
        const resolved = current.shuffleOrder[current.currentIndex] ?? 0;
        return {
          ...current,
          shuffled: false,
          shuffleOrder: [],
          currentIndex: resolved,
        };
      }
      const shuffleOrder = shuffleIndices(current.items.length);
      const resolved = current.shuffled
        ? current.currentIndex
        : current.currentIndex;
      const newCurrentIndex = shuffleOrder.findIndex(
        (index) => index === resolved,
      );
      return {
        ...current,
        shuffled: true,
        shuffleOrder,
        currentIndex: newCurrentIndex >= 0 ? newCurrentIndex : 0,
      };
    });
  }, []);

  const value = useMemo<VideoPlayerContextValue>(
    () => ({
      mode,
      queue,
      currentItem,
      playing,
      setPlaying,
      playEntity,
      playQueue,
      playNext,
      playPrevious,
      toggleShuffle,
      openFull: () => {
        if (currentItem) {
          setMode("full");
          navigate(`/entities/${currentItem.entityId}`);
        }
      },
      collapseToMini: () => {
        if (!currentItem) return;
        setMode("mini");
        navigate(returnRouteRef.current);
      },
      closePlayer: () => {
        setQueue((current) => {
          const item = currentQueueItem(current);
          if (item) {
            clearedPlaybackAssetsRef.current.add(item.primaryAssetId);
            playbackPositionsRef.current.delete(item.primaryAssetId);
          }
          return emptyQueue;
        });
        setPlaying(false);
        setMode("hidden");
        setActiveEntityAssets([]);
      },
      seekVersion,
      notifySeek: () => setSeekVersion((version) => version + 1),
      savePlaybackPosition,
      getPlaybackPosition,
      fullPlayerAnchor,
      registerFullPlayerAnchor,
      miniPlayerDock,
      registerMiniPlayerDock,
      miniCorner,
      setMiniCorner,
      miniDragOffset,
      setMiniDragOffset,
      activeEntityAssets,
      setActiveEntityAssets,
    }),
    [
      mode,
      queue,
      currentItem,
      playing,
      playEntity,
      playQueue,
      playNext,
      playPrevious,
      toggleShuffle,
      navigate,
      seekVersion,
      fullPlayerAnchor,
      registerFullPlayerAnchor,
      miniPlayerDock,
      registerMiniPlayerDock,
      miniCorner,
      setMiniCorner,
      miniDragOffset,
      activeEntityAssets,
      savePlaybackPosition,
      getPlaybackPosition,
    ],
  );

  return (
    <VideoPlayerContext.Provider value={value}>
      {children}
    </VideoPlayerContext.Provider>
  );
}

export function useVideoPlayer() {
  const ctx = useContext(VideoPlayerContext);
  if (!ctx) {
    throw new Error("useVideoPlayer must be used within VideoPlayerProvider");
  }
  return ctx;
}
