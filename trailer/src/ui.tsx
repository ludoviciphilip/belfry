import React from "react";
import { AbsoluteFill, Img, interpolate, spring, staticFile, useCurrentFrame, useVideoConfig, Easing } from "remotion";
import { loadFont as loadInter } from "@remotion/google-fonts/Inter";
import { loadFont as loadMono } from "@remotion/google-fonts/JetBrainsMono";

export const inter = loadInter("normal", { weights: ["400", "500", "600", "700", "800"], subsets: ["latin", "latin-ext"] }).fontFamily;
export const mono = loadMono("normal", { weights: ["400", "600"], subsets: ["latin"] }).fontFamily;

export const C = {
  bg: "#0e0c10",
  bg2: "#151219",
  card: "#1a161f",
  card2: "#221d28",
  line: "rgba(255,255,255,0.08)",
  line2: "rgba(255,255,255,0.13)",
  text: "#eee8f2",
  muted: "#9b91a6",
  faint: "#6c6476",
  accent: "#f0a84a",
  accent2: "#ffcf85",
  ink: "#261505",
  good: "#5fd39a",
  warn: "#e8c15a",
  bad: "#ff6f6f",
};

export const clamp = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;

export function useAppear(start: number, damping = 16) {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  return spring({ frame: f - start, fps, config: { damping, mass: 0.7 } });
}

/** Text that rises in word by word. */
export const Rise: React.FC<{ text: string; start: number; size: number; weight?: number; color?: string; stagger?: number; style?: React.CSSProperties; gradient?: boolean }> = ({
  text, start, size, weight = 750, color = C.text, stagger = 3, style, gradient,
}) => {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const words = text.split(" ");
  return (
    <div style={{ fontFamily: inter, fontSize: size, fontWeight: weight, lineHeight: 1.12, letterSpacing: "-0.02em", color, ...style }}>
      {words.map((w, i) => {
        const p = spring({ frame: f - start - i * stagger, fps, config: { damping: 15, mass: 0.6 } });
        return (
          <span key={i} style={{ display: "inline-block", opacity: p, transform: `translateY(${(1 - p) * size * 0.45}px)`, marginRight: size * 0.26,
            ...(gradient ? { background: `linear-gradient(180deg, #fff3dc, ${C.accent})`, WebkitBackgroundClip: "text", backgroundClip: "text", color: "transparent" } : {}) }}>
            {w}
          </span>
        );
      })}
    </div>
  );
};

/** Background: warm glow + slow drifting dust. */
export const Backdrop: React.FC = () => {
  const f = useCurrentFrame();
  const { width, height } = useVideoConfig();
  const dots = Array.from({ length: 46 }, (_, i) => {
    const seed = Math.sin(i * 91.7) * 10000;
    const r = seed - Math.floor(seed);
    const x = ((i * 137.5) % 100) / 100;
    const y0 = r;
    const y = (y0 - f * (0.0004 + r * 0.0007) + 10) % 1;
    return { x: x * width, y: y * height, s: 1.5 + r * 3, o: 0.08 + r * 0.22 };
  });
  return (
    <AbsoluteFill style={{ background: C.bg }}>
      <AbsoluteFill style={{ background: `radial-gradient(${width * 0.9}px ${height * 0.7}px at 50% -10%, rgba(240,168,74,0.16), transparent 70%)` }} />
      <AbsoluteFill style={{ background: `radial-gradient(${width * 0.6}px ${height * 0.5}px at 85% 110%, rgba(150,90,200,0.10), transparent 70%)` }} />
      {dots.map((d, i) => (
        <div key={i} style={{ position: "absolute", left: d.x, top: d.y, width: d.s, height: d.s, borderRadius: 99, background: C.accent2, opacity: d.o }} />
      ))}
    </AbsoluteFill>
  );
};

/** Expanding rings, the "sound of the bell" motif. */
export const Rings: React.FC<{ at: number[]; x: number; y: number; max?: number }> = ({ at, x, y, max = 900 }) => {
  const f = useCurrentFrame();
  return (
    <AbsoluteFill style={{ pointerEvents: "none" }}>
      {at.flatMap((t, k) =>
        [0, 9, 18].map((d, j) => {
          const p = interpolate(f - t - d, [0, 70], [0, 1], clamp);
          if (p <= 0 || p >= 1) return null;
          const r = p * max;
          return (
            <div key={`${k}-${j}`} style={{ position: "absolute", left: x - r, top: y - r, width: r * 2, height: r * 2, borderRadius: "50%",
              border: `${3 - j}px solid ${C.accent}`, opacity: (1 - p) * 0.45 }} />
          );
        })
      )}
    </AbsoluteFill>
  );
};

export const Bell: React.FC<{ size: number; swing?: number }> = ({ size, swing = 0 }) => (
  <Img src={staticFile("bell.svg")} style={{ width: size, height: size, transform: `rotate(${swing}deg)`, transformOrigin: "50% 12%", filter: "drop-shadow(0 10px 40px rgba(240,168,74,0.45))" }} />
);

/** Belfry window chrome. */
export const AppWindow: React.FC<{ w: number; h: number; children: React.ReactNode; style?: React.CSSProperties }> = ({ w, h, children, style }) => (
  <div style={{ width: w, height: h, borderRadius: 26, overflow: "hidden", position: "relative",
    background: `radial-gradient(${w}px ${h * 0.5}px at 50% -25%, rgba(240,168,74,0.14), transparent 70%), ${C.bg}`,
    border: `1px solid ${C.line2}`, boxShadow: "0 40px 120px rgba(0,0,0,0.6), 0 0 0 1px rgba(0,0,0,0.4)", fontFamily: inter, color: C.text, ...style }}>
    <div style={{ display: "flex", alignItems: "center", gap: 14, padding: "26px 30px 14px" }}>
      <Img src={staticFile("bell.svg")} style={{ width: 40, height: 40 }} />
      <span style={{ fontSize: 27, fontWeight: 700, background: "linear-gradient(180deg,#fff3dc,#f0b35a)", WebkitBackgroundClip: "text", backgroundClip: "text", color: "transparent" }}>Belfry</span>
    </div>
    <div style={{ padding: "6px 24px" }}>{children}</div>
  </div>
);

export const Dot: React.FC<{ state: "online" | "starting" }> = ({ state }) => {
  const f = useCurrentFrame();
  const col = state === "online" ? C.good : C.warn;
  return <span style={{ width: 14, height: 14, borderRadius: 99, background: col, flex: "none", opacity: state === "starting" ? 0.5 + 0.5 * Math.sin(f / 4) : 1,
    boxShadow: state === "online" ? "0 0 0 6px rgba(95,211,154,0.15)" : "none" }} />;
};

export type MemberT = { name: string; ip: string; me?: boolean; host?: boolean; ms?: number; fam?: string; at: number };

export const Member: React.FC<{ m: MemberT; t: Record<string, any> }> = ({ m, t }) => {
  const p = useAppear(m.at);
  const hue = [...m.name].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 0);
  const lat = m.ms ?? 0;
  const latCol = lat < 60 ? C.good : lat < 140 ? C.warn : C.bad;
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 14, padding: "9px 8px", opacity: p, transform: `translateX(${(1 - p) * 30}px)` }}>
      <span style={{ width: 40, height: 40, borderRadius: 12, display: "grid", placeItems: "center", fontWeight: 700, fontSize: 18,
        background: `hsl(${hue} 45% 26%)`, color: `hsl(${hue} 80% 85%)` }}>{m.name[0]}</span>
      <span style={{ flex: 1, display: "flex", alignItems: "center", gap: 9, fontSize: 24 }}>
        {m.name}
        {m.host && <Tag text={t.host} gold />}
        {m.me && <Tag text={t.you} />}
      </span>
      <span style={{ fontFamily: mono, fontSize: 19, color: C.muted }}>{m.ip}</span>
      {!m.me && (
        <>
          <span style={{ fontFamily: mono, fontSize: 17, color: latCol, width: 64, textAlign: "right" }}>{Math.round(lat)} ms</span>
          <span style={{ fontSize: 15, color: C.faint, width: 118 }}>{t.direct} · {m.fam ?? "IPv6"}</span>
        </>
      )}
    </div>
  );
};

export const Tag: React.FC<{ text: string; gold?: boolean }> = ({ text, gold }) => (
  <span style={{ fontSize: 14, fontWeight: 650, padding: "2px 9px", borderRadius: 8, background: gold ? "rgba(240,168,74,0.16)" : C.card2, color: gold ? C.accent2 : C.muted }}>{text}</span>
);

export const RoomCard: React.FC<{ name: string; t: Record<string, any>; ip: string; members: MemberT[]; state?: "online" | "starting"; showIp?: boolean }> = ({
  name, t, ip, members, state = "online", showIp = true,
}) => (
  <div style={{ background: `linear-gradient(180deg, ${C.card}, ${C.bg2})`, border: `1px solid ${state === "online" ? "rgba(95,211,154,0.2)" : C.line}`,
    borderRadius: 22, padding: "22px 22px 16px", boxShadow: "0 14px 40px rgba(0,0,0,0.3)" }}>
    <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
      <Dot state={state} />
      <div style={{ flex: 1 }}>
        <div style={{ fontSize: 28, fontWeight: 650 }}>{name}</div>
        <div style={{ fontSize: 16, color: C.muted }}><span style={{ color: C.accent2, fontWeight: 600 }}>{t.host}</span> · {t.online} · Luis</div>
      </div>
      <div style={{ width: 54, height: 32, borderRadius: 99, background: "rgba(240,168,74,0.25)", border: "1px solid rgba(240,168,74,0.5)", position: "relative" }}>
        <div style={{ position: "absolute", top: 5, left: 27, width: 20, height: 20, borderRadius: 99, background: C.accent2 }} />
      </div>
    </div>
    {showIp && (
      <div style={{ marginTop: 16, background: "rgba(0,0,0,0.22)", border: "1px solid rgba(240,168,74,0.25)", borderRadius: 16, padding: "10px 16px 12px" }}>
        <div style={{ fontSize: 13, color: C.muted, textTransform: "uppercase", letterSpacing: "0.07em" }}>{t.friendsUse}</div>
        <div style={{ fontFamily: mono, fontSize: 40, fontWeight: 600, color: C.accent2, marginTop: 2 }}>{ip}</div>
      </div>
    )}
    {members.length > 0 && (
      <>
        <div style={{ margin: "16px 4px 4px", fontSize: 13, color: C.faint, textTransform: "uppercase", letterSpacing: "0.09em" }}>{t.players} · {members.length}</div>
        {members.map((m) => <Member key={m.ip} m={m} t={t} />)}
      </>
    )}
  </div>
);

export function typed(text: string, start: number, cps: number, f: number) {
  const n = Math.max(0, Math.min(text.length, Math.floor(((f - start) / 30) * cps)));
  return text.slice(0, n);
}

export const Caret: React.FC = () => {
  const f = useCurrentFrame();
  return <span style={{ display: "inline-block", width: 3, height: "1em", background: C.accent, marginLeft: 3, verticalAlign: "-0.12em", opacity: Math.floor(f / 15) % 2 ? 0 : 1 }} />;
};

export const Btn: React.FC<{ text: string; primary?: boolean; pressed?: number; style?: React.CSSProperties }> = ({ text, primary, pressed = 0, style }) => (
  <div style={{ display: "inline-flex", alignItems: "center", justifyContent: "center", height: 58, padding: "0 28px", borderRadius: 16, fontWeight: 650, fontSize: 21,
    transform: `scale(${1 - pressed * 0.06})`,
    background: primary ? `linear-gradient(180deg, ${C.accent2}, ${C.accent})` : C.card2,
    color: primary ? C.ink : C.text, border: primary ? "none" : `1px solid ${C.line2}`,
    boxShadow: primary ? "0 8px 26px rgba(240,168,74,0.3)" : "none", ...style }}>{text}</div>
);

/** A press pulse: 0 -> 1 -> 0 around frame `at`. */
export function press(f: number, at: number) {
  return interpolate(f, [at - 3, at, at + 6], [0, 1, 0], clamp);
}

/** Mouse pointer gliding between points. */
export const Cursor: React.FC<{ path: { f: number; x: number; y: number }[] }> = ({ path }) => {
  const f = useCurrentFrame();
  const fs = path.map((p) => p.f);
  const x = interpolate(f, fs, path.map((p) => p.x), { ...clamp, easing: Easing.inOut(Easing.cubic) });
  const y = interpolate(f, fs, path.map((p) => p.y), { ...clamp, easing: Easing.inOut(Easing.cubic) });
  const o = interpolate(f, [fs[0], fs[0] + 6, fs[fs.length - 1], fs[fs.length - 1] + 8], [0, 1, 1, 0], clamp);
  return (
    <svg width="34" height="34" viewBox="0 0 24 24" style={{ position: "absolute", left: x, top: y, opacity: o, filter: "drop-shadow(0 3px 6px rgba(0,0,0,0.6))", zIndex: 50 }}>
      <path d="M4 2l16 9.5-7 1.6-3.6 6.9z" fill="#fff" stroke="#111" strokeWidth="1.2" strokeLinejoin="round" />
    </svg>
  );
};

export const Icon: React.FC<{ d: string; size: number; color?: string }> = ({ d, size, color = C.accent2 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24"><path d={d} fill={color} /></svg>
);

export const ICONS = {
  user: "M12 12a4.5 4.5 0 1 0 0-9 4.5 4.5 0 0 0 0 9Zm0 2c-4 0-8 2-8 5v2h16v-2c0-3-4-5-8-5Z",
  users: "M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8Zm7.5-1a3 3 0 1 0 0-6 3 3 0 0 0 0 6ZM9 13c-3.3 0-7 1.6-7 4.5V20h14v-2.5C16 14.6 12.3 13 9 13Zm7.5 0c-.5 0-1 0-1.5.1 1.2.9 2 2.2 2 3.9V20h5v-2.5c0-2.7-2.9-4.5-5.5-4.5Z",
  lock: "M17 9V7A5 5 0 0 0 7 7v2a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-9a2 2 0 0 0-2-2ZM9 7a3 3 0 0 1 6 0v2H9V7Zm3 10a2 2 0 1 1 0-4 2 2 0 0 1 0 4Z",
  monitor: "M21 3H3a2 2 0 0 0-2 2v11a2 2 0 0 0 2 2h7v2H8v2h8v-2h-2v-2h7a2 2 0 0 0 2-2V5a2 2 0 0 0-2-2Zm0 13H3V5h18v11Z",
  code: "M9.4 16.6 4.8 12l4.6-4.6L8 6l-6 6 6 6 1.4-1.4Zm5.2 0 4.6-4.6-4.6-4.6L16 6l6 6-6 6-1.4-1.4Z",
  refresh: "M17.65 6.35A7.96 7.96 0 0 0 12 4a8 8 0 1 0 7.73 10h-2.08A6 6 0 1 1 12 6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35Z",
  laptop: "M20 18c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2H4c-1.1 0-2 .9-2 2v10c0 1.1.9 2 2 2H0v2h24v-2h-4ZM4 6h16v10H4V6Z",
  file: "M14 2H6a2 2 0 0 0-2 2v16c0 1.1.9 2 2 2h12a2 2 0 0 0 2-2V8l-6-6Zm4 18H6V4h7v5h5v11Z",
  x: "M19 6.4 17.6 5 12 10.6 6.4 5 5 6.4 10.6 12 5 17.6 6.4 19 12 13.4 17.6 19 19 17.6 13.4 12 19 6.4Z",
};
