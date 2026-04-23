import { useEffect, useState } from "react";

// Source-of-truth ASCII frames are the unmodified .txt files copied verbatim
// from hugomd/parrot.live's `frames/` directory. Imported as raw strings via
// Vite's `?raw` suffix so the leading-whitespace alignment can't drift.
import f0 from "../parrot/0.txt?raw";
import f1 from "../parrot/1.txt?raw";
import f2 from "../parrot/2.txt?raw";
import f3 from "../parrot/3.txt?raw";
import f4 from "../parrot/4.txt?raw";
import f5 from "../parrot/5.txt?raw";
import f6 from "../parrot/6.txt?raw";
import f7 from "../parrot/7.txt?raw";
import f8 from "../parrot/8.txt?raw";
import f9 from "../parrot/9.txt?raw";

const FRAMES: string[] = [f0, f1, f2, f3, f4, f5, f6, f7, f8, f9];

// parrot.live cycles frames at ~70ms; matching that gives the same dance feel.
const FRAME_INTERVAL_MS = 70;

// Step the rainbow hue 36° per frame so a full color cycle finishes once per
// 10-frame loop — the classic party-parrot effect.
const HUE_STEP = 36;

export function ParrotAnimation() {
  const [tick, setTick] = useState(0);

  useEffect(() => {
    const id = window.setInterval(() => {
      setTick((t) => (t + 1) % FRAMES.length);
    }, FRAME_INTERVAL_MS);
    return () => window.clearInterval(id);
  }, []);

  const frame = FRAMES[tick];
  const hue = (tick * HUE_STEP) % 360;

  return (
    <div className="parrot-wrap">
      <pre
        className="parrot-art"
        style={{ color: `hsl(${hue}, 90%, 65%)` }}
      >
        {frame}
      </pre>
      <div className="parrot-caption">party parrot</div>
    </div>
  );
}
