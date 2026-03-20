import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

type OverlayState = "recording" | "transcribing";

const COL_WIDTH = 3;
const COL_GAP = 2;
const CANVAS_HEIGHT = 32;
const SAMPLE_EVERY_N_FRAMES = 3;
const AMPLITUDE_SCALE = 8;

function Overlay() {
  const [state, setState] = useState<OverlayState>("recording");
  const levelRef = useRef(0);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const historyRef = useRef<number[]>([]);
  const frameCountRef = useRef(0);
  const animRef = useRef<number>(0);
  const saveTimerRef = useRef<number>(0);

  useEffect(() => {
    const unlisten1 = listen<number>("audio-level", (e) => {
      levelRef.current = e.payload;
    });
    const unlisten2 = listen("transcribing", () => {
      setState("transcribing");
    });
    return () => {
      unlisten1.then((f) => f());
      unlisten2.then((f) => f());
    };
  }, []);

  // Persist overlay position on drag (debounced)
  useEffect(() => {
    const currentWindow = getCurrentWindow();
    const unlisten = currentWindow.onMoved(async () => {
      clearTimeout(saveTimerRef.current);
      saveTimerRef.current = window.setTimeout(async () => {
        try {
          const [pos, scale] = await Promise.all([
            currentWindow.outerPosition(),
            currentWindow.scaleFactor(),
          ]);
          invoke("save_overlay_position", {
            x: pos.x / scale,
            y: pos.y / scale,
          });
        } catch {
          // ignore errors during window close
        }
      }, 300);
    });
    return () => {
      unlisten.then((f) => f());
      clearTimeout(saveTimerRef.current);
    };
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || state !== "recording") return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const canvasWidth = 280;
    const historyLength = Math.floor(canvasWidth / (COL_WIDTH + COL_GAP));
    historyRef.current = new Array(historyLength).fill(0);
    frameCountRef.current = 0;

    const dpr = window.devicePixelRatio || 1;
    canvas.width = canvasWidth * dpr;
    canvas.height = CANVAS_HEIGHT * dpr;
    ctx.scale(dpr, dpr);

    const draw = () => {
      const level = levelRef.current;
      const amplitude = Math.min(1, level * AMPLITUDE_SCALE);

      frameCountRef.current++;
      if (frameCountRef.current >= SAMPLE_EVERY_N_FRAMES) {
        frameCountRef.current = 0;
        const history = historyRef.current;
        history.push(amplitude);
        if (history.length > historyLength) {
          history.shift();
        }
      }

      ctx.clearRect(0, 0, canvasWidth, CANVAS_HEIGHT);
      const midY = CANVAS_HEIGHT / 2;
      const history = historyRef.current;
      const maxHalfH = CANVAS_HEIGHT / 2 - 2;
      const radius = COL_WIDTH / 2;
      const isDark = window.matchMedia("(prefers-color-scheme: dark)").matches;

      for (let i = 0; i < history.length; i++) {
        const amp = history[i];
        const halfH = Math.max(1.5, amp * maxHalfH);
        const x = i * (COL_WIDTH + COL_GAP);
        const alpha = 0.35 + amp * 0.6;
        ctx.fillStyle = isDark
          ? `rgba(255, 255, 255, ${alpha})`
          : `rgba(0, 0, 0, ${alpha * 0.85})`;
        const barY = midY - halfH;
        const barH = halfH * 2;
        ctx.beginPath();
        ctx.roundRect(x, barY, COL_WIDTH, barH, radius);
        ctx.fill();
      }

      animRef.current = requestAnimationFrame(draw);
    };

    draw();
    return () => cancelAnimationFrame(animRef.current);
  }, [state]);

  const handlePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button === 0) {
      getCurrentWindow().startDragging();
    }
  };

  return (
    <div className="overlay-body" onPointerDown={handlePointerDown}>
      {state === "transcribing" ? (
        <div className="transcribing-dots" style={{ width: 280, height: CANVAS_HEIGHT }}>
          <span className="dot" />
          <span className="dot" />
          <span className="dot" />
          <span className="dot" />
          <span className="dot" />
        </div>
      ) : (
        <canvas
          ref={canvasRef}
          className="wave-canvas"
          style={{ width: 280, height: CANVAS_HEIGHT }}
        />
      )}
    </div>
  );
}

export default Overlay;
