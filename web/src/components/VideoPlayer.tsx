import { useEffect, useRef, useState, type CSSProperties, type PointerEvent } from "react";
import { createPortal } from "react-dom";
import { useVault } from "../context/VaultContext";
import { useVideoPlayer } from "../context/VideoPlayerContext";
import { formatDurationLabel } from "../player/queue";
import {
  pointerMovedBeyondThreshold,
  snapMiniCorner,
} from "../player/miniPlayerSnap";
import PlayerSettingsMenu from "./player/PlayerSettingsMenu";
import type { SettingsPanel } from "./player/playerSettings";
import {
  IconClose,
  IconExpand,
  IconFullscreen,
  IconNext,
  IconPause,
  IconPlay,
  IconPrev,
  IconSettings,
  IconShuffle,
  IconVolume,
  IconVolumeMuted,
} from "./player/PlayerIcons";

const IDLE_HIDE_MS = 3000;

function capturePointer(target: HTMLElement, pointerId: number) {
  if (typeof target.setPointerCapture === "function") {
    target.setPointerCapture(pointerId);
  }
}

function releasePointer(target: HTMLElement, pointerId: number) {
  if (
    typeof target.hasPointerCapture === "function" &&
    target.hasPointerCapture(pointerId)
  ) {
    target.releasePointerCapture(pointerId);
  }
}

export default function VideoPlayer() {
  const { assetContentUrl } = useVault();
  const {
    mode,
    currentItem,
    playing,
    setPlaying,
    playNext,
    playPrevious,
    queue,
    toggleShuffle,
    openFull,
    closePlayer,
    collapseToMini,
    notifySeek,
    savePlaybackPosition,
    getPlaybackPosition,
    fullPlayerAnchor,
    miniPlayerDock,
    setMiniCorner,
    setMiniDragOffset,
    activeEntityAssets,
  } = useVideoPlayer();
  const videoRef = useRef<HTMLVideoElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const miniStageRef = useRef<HTMLDivElement>(null);
  const settingsRef = useRef<HTMLDivElement>(null);
  const idleTimerRef = useRef<number | null>(null);
  const miniPointerRef = useRef<{
    startX: number;
    startY: number;
    dragging: boolean;
    didDrag: boolean;
  } | null>(null);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(1);
  const [muted, setMuted] = useState(false);
  const [playbackRate, setPlaybackRate] = useState(1);
  const [hovered, setHovered] = useState(false);
  const [idleHidden, setIdleHidden] = useState(false);
  const [buffering, setBuffering] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [autoplayBlocked, setAutoplayBlocked] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsPanel, setSettingsPanel] = useState<SettingsPanel>("main");
  const [centerFlash, setCenterFlash] = useState<"play" | "pause" | null>(null);
  const [miniDragging, setMiniDragging] = useState(false);
  const [mediaAspectRatio, setMediaAspectRatio] = useState(16 / 9);

  const variant = mode === "mini" ? "mini" : "full";
  const mediaSrc = currentItem
    ? assetContentUrl(currentItem.primaryAssetId)
    : undefined;

  useEffect(() => {
    if (!currentItem) return;
    const saved = getPlaybackPosition(currentItem.primaryAssetId);
    setCurrentTime(saved);
    setDuration(0);
    setBuffering(false);
    setError(null);
    setAutoplayBlocked(false);
    setSettingsOpen(false);
    setSettingsPanel("main");
    setIdleHidden(false);
    setMediaAspectRatio(16 / 9);
  }, [currentItem?.primaryAssetId, getPlaybackPosition]);

  function restorePlaybackPosition() {
    const video = videoRef.current;
    if (!video || !currentItem) return;
    const saved = getPlaybackPosition(currentItem.primaryAssetId);
    if (
      saved > 0.25 &&
      Number.isFinite(video.duration) &&
      video.duration > 0 &&
      saved < video.duration - 0.25
    ) {
      video.currentTime = saved;
      setCurrentTime(saved);
    }
  }

  const portalReady =
    mode === "full"
      ? fullPlayerAnchor !== null
      : mode === "mini"
        ? miniPlayerDock !== null
        : false;

  useEffect(() => {
    if (!portalReady) return;
    const video = videoRef.current;
    if (!video) return;
    if (video.readyState >= HTMLMediaElement.HAVE_METADATA) {
      restorePlaybackPosition();
      return;
    }
    const handleLoaded = () => restorePlaybackPosition();
    video.addEventListener("loadedmetadata", handleLoaded, { once: true });
    return () => video.removeEventListener("loadedmetadata", handleLoaded);
  }, [portalReady, mode, currentItem?.primaryAssetId, getPlaybackPosition]);

  useEffect(() => {
    const video = videoRef.current;
    if (!video || !portalReady) return;
    if (playing) {
      const attemptPlay = () => {
        restorePlaybackPosition();
        void video.play().catch(() => {
          setPlaying(false);
          setAutoplayBlocked(true);
        });
      };
      if (video.readyState >= HTMLMediaElement.HAVE_METADATA) {
        attemptPlay();
      } else {
        video.addEventListener("loadedmetadata", attemptPlay, { once: true });
        return () => video.removeEventListener("loadedmetadata", attemptPlay);
      }
    } else {
      video.pause();
    }
  }, [playing, setPlaying, currentItem?.primaryAssetId, mediaSrc, portalReady, mode]);

  useEffect(() => {
    return () => {
      const video = videoRef.current;
      if (video && currentItem) {
        savePlaybackPosition(currentItem.primaryAssetId, video.currentTime);
      }
    };
  }, [currentItem?.primaryAssetId, portalReady, mode, savePlaybackPosition]);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    video.volume = volume;
    video.muted = muted;
    video.playbackRate = playbackRate;
  }, [volume, muted, playbackRate]);

  useEffect(() => {
    if (!settingsOpen) return;
    function handlePointerDown(event: MouseEvent) {
      if (!settingsRef.current?.contains(event.target as Node)) {
        setSettingsOpen(false);
        setSettingsPanel("main");
      }
    }
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setSettingsOpen(false);
        setSettingsPanel("main");
      }
    }
    document.addEventListener("mousedown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [settingsOpen]);

  useEffect(() => {
    if (!centerFlash) return;
    const timer = window.setTimeout(() => setCenterFlash(null), 600);
    return () => window.clearTimeout(timer);
  }, [centerFlash]);

  function resetIdleTimer() {
    setIdleHidden(false);
    if (idleTimerRef.current !== null) {
      window.clearTimeout(idleTimerRef.current);
    }
    if (playing && !settingsOpen && !autoplayBlocked) {
      idleTimerRef.current = window.setTimeout(() => {
        setIdleHidden(true);
        setHovered(false);
      }, IDLE_HIDE_MS);
    }
  }

  function handleStageMouseMove() {
    setHovered(true);
    resetIdleTimer();
  }

  function handleStageMouseLeave() {
    setHovered(false);
    if (idleTimerRef.current !== null) {
      window.clearTimeout(idleTimerRef.current);
      idleTimerRef.current = null;
    }
    if (playing) {
      setIdleHidden(true);
    }
  }

  useEffect(() => {
    if (playing) {
      resetIdleTimer();
    } else {
      setIdleHidden(false);
      if (idleTimerRef.current !== null) {
        window.clearTimeout(idleTimerRef.current);
        idleTimerRef.current = null;
      }
    }
    return () => {
      if (idleTimerRef.current !== null) {
        window.clearTimeout(idleTimerRef.current);
      }
    };
  }, [playing, settingsOpen, autoplayBlocked]);

  if (!currentItem || mode === "hidden") return null;

  const hasQueue = queue.items.length > 1;
  const showControls =
    settingsOpen ||
    autoplayBlocked ||
    (playing ? hovered && !idleHidden : hovered);
  const seekMax = duration || 0;
  const seekValue = seekMax > 0 ? Math.min(currentTime, seekMax) : 0;
  const volumeValue = muted ? 0 : volume;
  const hideCursor = playing && idleHidden && !settingsOpen;

  function seekTo(value: number) {
    const video = videoRef.current;
    if (!video || !Number.isFinite(value) || !currentItem) return;
    video.currentTime = value;
    setCurrentTime(value);
    savePlaybackPosition(currentItem.primaryAssetId, value);
    notifySeek();
  }

  function togglePlay() {
    setAutoplayBlocked(false);
    const nextPlaying = !playing;
    setCenterFlash(nextPlaying ? "play" : "pause");
    setPlaying(nextPlaying);
  }

  function toggleMute() {
    setMuted((current) => !current);
  }

  function toggleFullscreen() {
    const target = stageRef.current;
    if (!target) return;
    if (document.fullscreenElement) {
      void document.exitFullscreen();
    } else {
      void target.requestFullscreen();
    }
  }

  function handleMiniPointerDown(event: PointerEvent<HTMLDivElement>) {
    if ((event.target as HTMLElement).closest("button")) return;
    miniPointerRef.current = {
      startX: event.clientX,
      startY: event.clientY,
      dragging: false,
      didDrag: false,
    };
    capturePointer(event.currentTarget, event.pointerId);
  }

  function handleMiniPointerMove(event: PointerEvent<HTMLDivElement>) {
    const pointer = miniPointerRef.current;
    if (!pointer) return;

    if (
      !pointer.dragging &&
      pointerMovedBeyondThreshold(
        pointer.startX,
        pointer.startY,
        event.clientX,
        event.clientY,
      )
    ) {
      pointer.dragging = true;
      pointer.didDrag = true;
      setMiniDragging(true);
    }

    if (pointer.dragging) {
      setMiniDragOffset({
        x: event.clientX - pointer.startX,
        y: event.clientY - pointer.startY,
      });
    }
  }

  function handleMiniPointerUp(event: PointerEvent<HTMLDivElement>) {
    const pointer = miniPointerRef.current;
    if (!pointer) return;

    if (pointer.dragging) {
      const rect = miniStageRef.current?.getBoundingClientRect();
      const centerX = rect ? rect.left + rect.width / 2 : event.clientX;
      const centerY = rect ? rect.top + rect.height / 2 : event.clientY;
      setMiniCorner(
        snapMiniCorner(centerX, centerY, window.innerWidth, window.innerHeight),
      );
      setMiniDragOffset({ x: 0, y: 0 });
    } else if (!pointer.didDrag) {
      openFull();
    }

    miniPointerRef.current = null;
    setMiniDragging(false);
    releasePointer(event.currentTarget, event.pointerId);
  }

  function handleMiniPointerCancel(event: PointerEvent<HTMLDivElement>) {
    miniPointerRef.current = null;
    setMiniDragging(false);
    setMiniDragOffset({ x: 0, y: 0 });
    releasePointer(event.currentTarget, event.pointerId);
  }

  function renderVideoElement(onVideoClick?: () => void) {
    if (!currentItem) return null;
    return (
      <video
        key={currentItem.primaryAssetId}
        ref={videoRef}
        className="video-player-media"
        src={mediaSrc}
        playsInline
        preload="auto"
        onTimeUpdate={(event) => {
          const time = event.currentTarget.currentTime;
          setCurrentTime(time);
          savePlaybackPosition(currentItem.primaryAssetId, time);
        }}
        onLoadedMetadata={(event) => {
          const video = event.currentTarget;
          if (video.videoWidth > 0 && video.videoHeight > 0) {
            setMediaAspectRatio(video.videoWidth / video.videoHeight);
          }
          setDuration(video.duration);
          restorePlaybackPosition();
        }}
        onSeeking={() => setBuffering(true)}
        onSeeked={() => setBuffering(false)}
        onWaiting={() => setBuffering(true)}
        onCanPlay={() => setBuffering(false)}
        onError={() => {
          setError("Playback failed");
          setPlaying(false);
          setBuffering(false);
        }}
        onEnded={() => {
          if (hasQueue) playNext();
          else setPlaying(false);
        }}
        onClick={onVideoClick}
      />
    );
  }

  const miniProgress =
    duration > 0 ? Math.min(100, (currentTime / duration) * 100) : 0;
  const miniShowControls = hovered || !playing;

  const miniPlayerBody = (
    <div className="video-player video-player-mini">
      <div
        ref={miniStageRef}
        className={`video-player-stage video-player-mini-stage${miniDragging ? " dragging" : ""}`}
        style={{ aspectRatio: mediaAspectRatio }}
        onPointerDown={handleMiniPointerDown}
        onPointerMove={handleMiniPointerMove}
        onPointerUp={handleMiniPointerUp}
        onPointerCancel={handleMiniPointerCancel}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
        onFocusCapture={() => setHovered(true)}
        onBlurCapture={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
            setHovered(false);
          }
        }}
      >
        {renderVideoElement()}

        {buffering ? <div className="video-player-buffering" aria-hidden="true" /> : null}
        {error ? <div className="video-player-error">{error}</div> : null}

        <div
          className={`video-player-mini-controls${miniShowControls ? " visible" : ""}`}
        >
          <button
            type="button"
            className="video-player-icon-button video-player-mini-button video-player-mini-button-play"
            aria-label={playing ? "Pause" : "Play"}
            onPointerDown={(event) => event.stopPropagation()}
            onClick={(event) => {
              event.stopPropagation();
              togglePlay();
            }}
          >
            {playing ? <IconPause /> : <IconPlay />}
          </button>
          <button
            type="button"
            className="video-player-icon-button video-player-mini-button video-player-mini-button-close"
            aria-label="Close"
            onPointerDown={(event) => event.stopPropagation()}
            onClick={(event) => {
              event.stopPropagation();
              closePlayer();
            }}
          >
            <IconClose />
          </button>
        </div>

        <div
          className="video-player-mini-progress"
          aria-hidden="true"
          style={{ "--mini-progress": `${miniProgress}%` } as CSSProperties}
        />
      </div>
    </div>
  );

  const fullPlayerBody = (
    <div className={`video-player video-player-${variant}`}>
      <div
        ref={stageRef}
        className={`video-player-stage${hideCursor ? " cursor-hidden" : ""}`}
        onMouseMove={handleStageMouseMove}
        onMouseLeave={handleStageMouseLeave}
        onFocusCapture={() => setHovered(true)}
        onBlurCapture={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
            setHovered(false);
          }
        }}
      >
        {renderVideoElement(togglePlay)}

        {autoplayBlocked && !playing ? (
          <button
            type="button"
            className="video-player-center-play"
            aria-label="Play"
            onClick={togglePlay}
          >
            <IconPlay />
          </button>
        ) : null}

        {centerFlash ? (
          <div className="video-player-center-flash" aria-hidden="true">
            {centerFlash === "play" ? <IconPlay /> : <IconPause />}
          </div>
        ) : null}

        {buffering ? <div className="video-player-buffering" aria-hidden="true" /> : null}
        {error ? <div className="video-player-error">{error}</div> : null}

        <div className={`video-player-overlay${showControls ? " visible" : ""}`}>
          <input
            className="video-player-seek"
            type="range"
            min={0}
            max={seekMax}
            step={0.1}
            value={seekValue}
            aria-label="Seek"
            style={
              seekMax > 0
                ? ({
                    "--seek-progress": `${(seekValue / seekMax) * 100}%`,
                  } as CSSProperties)
                : undefined
            }
            onChange={(event) => seekTo(Number(event.target.value))}
          />

          <div className="video-player-toolbar">
            <div className="video-player-left">
              {hasQueue ? (
                <button
                  type="button"
                  className="video-player-icon-button"
                  aria-label="Previous"
                  onClick={playPrevious}
                >
                  <IconPrev />
                </button>
              ) : null}
              <button
                type="button"
                className="video-player-icon-button"
                aria-label={playing ? "Pause" : "Play"}
                onClick={togglePlay}
              >
                {playing ? <IconPause /> : <IconPlay />}
              </button>
              {hasQueue ? (
                <button
                  type="button"
                  className="video-player-icon-button"
                  aria-label="Next"
                  onClick={playNext}
                >
                  <IconNext />
                </button>
              ) : null}

              <div className="video-player-volume">
                <button
                  type="button"
                  className="video-player-icon-button"
                  aria-label={muted || volume === 0 ? "Unmute" : "Mute"}
                  onClick={toggleMute}
                >
                  {muted || volume === 0 ? <IconVolumeMuted /> : <IconVolume />}
                </button>
                <input
                  className="video-player-volume-slider"
                  type="range"
                  min={0}
                  max={1}
                  step={0.05}
                  value={volumeValue}
                  aria-label="Volume"
                  style={{
                    background: `linear-gradient(to right, rgba(255,255,255,0.9) 0%, rgba(255,255,255,0.9) ${volumeValue * 100}%, rgba(255,255,255,0.3) ${volumeValue * 100}%, rgba(255,255,255,0.3) 100%)`,
                  }}
                  onChange={(event) => {
                    const next = Number(event.target.value);
                    setVolume(next);
                    setMuted(next === 0);
                  }}
                />
              </div>

              <div className="video-player-time">
                {formatDurationLabel(null, currentTime)} /{" "}
                {formatDurationLabel(currentItem.duration, duration)}
              </div>
            </div>

            <div className="video-player-right">
              {hasQueue ? (
                <button
                  type="button"
                  className={`video-player-icon-button${queue.shuffled ? " active" : ""}`}
                  aria-label="Shuffle"
                  onClick={toggleShuffle}
                >
                  <IconShuffle />
                </button>
              ) : null}

              <div className="video-player-settings" ref={settingsRef}>
                <button
                  type="button"
                  className={`video-player-icon-button${settingsOpen ? " active" : ""}`}
                  aria-label="Settings"
                  aria-expanded={settingsOpen}
                  onClick={() => {
                    setSettingsOpen((open) => {
                      if (open) setSettingsPanel("main");
                      return !open;
                    });
                  }}
                >
                  <IconSettings />
                </button>
                {settingsOpen ? (
                  <PlayerSettingsMenu
                    assets={activeEntityAssets}
                    playbackRate={playbackRate}
                    settingsPanel={settingsPanel}
                    onPanelChange={setSettingsPanel}
                    onPlaybackRateChange={setPlaybackRate}
                    onClose={() => {
                      setSettingsOpen(false);
                      setSettingsPanel("main");
                    }}
                  />
                ) : null}
              </div>

              {variant === "full" ? (
                <button
                  type="button"
                  className="video-player-icon-button"
                  aria-label="Mini player"
                  onClick={collapseToMini}
                >
                  <IconExpand />
                </button>
              ) : null}

              {variant === "full" ? (
                <button
                  type="button"
                  className="video-player-icon-button"
                  aria-label="Fullscreen"
                  onClick={toggleFullscreen}
                >
                  <IconFullscreen />
                </button>
              ) : null}
            </div>
          </div>
        </div>
      </div>
    </div>
  );

  if (mode === "full") {
    if (!fullPlayerAnchor) return null;
    return createPortal(fullPlayerBody, fullPlayerAnchor);
  }

  if (mode === "mini" && miniPlayerDock) {
    return createPortal(miniPlayerBody, miniPlayerDock);
  }

  return null;
}
