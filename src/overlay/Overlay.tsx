import { useState, useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

type OverlayState = "recording" | "transcribing";

const COL_WIDTH = 2;
const COL_GAP = 1;
const CANVAS_HEIGHT = 40;
const SAMPLE_EVERY_N_FRAMES = 4;

function Overlay() {
  const [state, setState] = useState<OverlayState>("recording");
  const levelRef = useRef(0);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const historyRef = useRef<number[]>([]);
  const frameCountRef = useRef(0);
  const animRef = useRef<number>(0);

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

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || state !== "recording") return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const canvasWidth = 300;
    const historyLength = Math.floor(canvasWidth / (COL_WIDTH + COL_GAP));
    historyRef.current = new Array(historyLength).fill(0);
    frameCountRef.current = 0;

    const dpr = window.devicePixelRatio || 1;
    canvas.width = canvasWidth * dpr;
    canvas.height = CANVAS_HEIGHT * dpr;
    ctx.scale(dpr, dpr);

    const draw = () => {
      const level = levelRef.current;
      const amplitude = Math.min(1, level * 3);

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

      ctx.fillStyle = "rgba(56, 189, 248, 0.75)";

      for (let i = 0; i < history.length; i++) {
        const amp = history[i];
        const halfH = Math.max(1, amp * (CANVAS_HEIGHT / 2 - 2));
        const x = i * (COL_WIDTH + COL_GAP);
        ctx.fillRect(x, midY - halfH, COL_WIDTH, halfH * 2);
      }

      animRef.current = requestAnimationFrame(draw);
    };

    draw();
    return () => cancelAnimationFrame(animRef.current);
  }, [state]);

  // Drag support — now works because transparent(true) is removed
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (e.buttons === 1) {
        getCurrentWindow().startDragging();
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  return (
    <div className="overlay-body">
      <canvas
        ref={canvasRef}
        className="wave-canvas"
        style={{ width: 300, height: CANVAS_HEIGHT }}
      />
    </div>
  );
}

export default Overlay;
