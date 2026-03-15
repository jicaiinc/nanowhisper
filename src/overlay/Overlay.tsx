import { useState, useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

type OverlayState = "recording" | "transcribing";

function Overlay() {
  const [state, setState] = useState<OverlayState>("recording");
  const levelRef = useRef(0);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const phaseRef = useRef(0);

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

    const ctx = canvas.getContext("2d")!;
    let animId: number;

    const draw = () => {
      const w = canvas.width;
      const h = canvas.height;
      ctx.clearRect(0, 0, w, h);

      const level = levelRef.current;
      const amplitude = Math.max(4, level * h * 0.4);
      phaseRef.current += 0.08;

      ctx.beginPath();
      ctx.moveTo(0, h / 2);
      for (let x = 0; x < w; x++) {
        const y =
          h / 2 +
          Math.sin((x / w) * Math.PI * 3 + phaseRef.current) * amplitude +
          Math.sin((x / w) * Math.PI * 5 + phaseRef.current * 1.3) * amplitude * 0.3;
        ctx.lineTo(x, y);
      }
      ctx.strokeStyle = "rgba(56, 189, 248, 0.8)";
      ctx.lineWidth = 2.5;
      ctx.stroke();

      animId = requestAnimationFrame(draw);
    };

    draw();
    return () => cancelAnimationFrame(animId);
  }, [state]);

  return (
    <div className="overlay-container">
      {state === "recording" ? (
        <canvas ref={canvasRef} width={320} height={48} className="wave-canvas" />
      ) : (
        <div className="transcribing-text">Transcribing...</div>
      )}
    </div>
  );
}

export default Overlay;
