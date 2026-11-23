import React from "react";
import { AbsoluteFill, Sequence, interpolate, spring, useCurrentFrame, useVideoConfig, Easing } from "remotion";
import { S, CODE, HOST_IP, Lang } from "./strings";
import {
  C, clamp, inter, mono, Rise, Backdrop, Rings, Bell, AppWindow, RoomCard, MemberT, typed, Caret, Btn, press, Cursor, Icon, ICONS, useAppear,
} from "./ui";

export const SCENES = {
  hook: [0, 105],
  pain: [105, 120],
  logo: [225, 105],
  create: [330, 210],
  share: [540, 210],
  play: [750, 180],
  chat: [930, 180],
  feats: [1110, 180],
  end: [1290, 125],
} as const;
export const TOTAL = 1415;

const Scene: React.FC<{ k: keyof typeof SCENES; children: React.ReactNode; fadeOut?: boolean }> = ({ k, children, fadeOut = true }) => {
  const [from, dur] = SCENES[k];
  return (
    <Sequence from={from} durationInFrames={dur}>
      <Fade dur={dur} fadeOut={fadeOut}>{children}</Fade>
    </Sequence>
  );
};

const Fade: React.FC<{ dur: number; fadeOut: boolean; children: React.ReactNode }> = ({ dur, fadeOut, children }) => {
  const f = useCurrentFrame();
  const o = interpolate(f, [0, 9, dur - 9, dur], [0, 1, 1, fadeOut ? 0 : 1], clamp);
  const s = interpolate(f, [0, 12], [1.025, 1], { ...clamp, easing: Easing.out(Easing.cubic) });
  return <AbsoluteFill style={{ opacity: o, transform: `scale(${s})` }}>{children}</AbsoluteFill>;
};

/** Step headline: number badge + title + subtitle. */
const Step: React.FC<{ n: number; title: string; sub: string; v: boolean }> = ({ n, title, sub, v }) => {
  const p = useAppear(2);
  const size = v ? 80 : 86;
  return (
    <div style={{ position: "absolute", left: v ? 80 : 130, right: v ? 80 : undefined, top: v ? 150 : 0, bottom: v ? undefined : 0, width: v ? undefined : 700,
      display: "flex", flexDirection: "column", justifyContent: "center", alignItems: v ? "center" : "flex-start", textAlign: v ? "center" : "left" }}>
      <div style={{ width: 72, height: 72, borderRadius: 22, display: "grid", placeItems: "center", fontFamily: inter, fontWeight: 800, fontSize: 38,
        background: `linear-gradient(180deg, ${C.accent2}, ${C.accent})`, color: C.ink, transform: `scale(${p})`, marginBottom: 26,
        boxShadow: "0 10px 30px rgba(240,168,74,0.35)" }}>{n}</div>
      <Rise text={title} start={6} size={size} style={{ justifyContent: v ? "center" : "flex-start", display: "flex", flexWrap: "wrap" }} />
      <Rise text={sub} start={18} size={v ? 40 : 40} weight={450} color={C.muted} stagger={1}
        style={{ marginTop: 18, maxWidth: v ? 900 : 660, display: "flex", flexWrap: "wrap", justifyContent: v ? "center" : "flex-start" }} />
    </div>
  );
};

/** Where the "product" lives: right half (H) or lower area (V). */
const Stage: React.FC<{ v: boolean; children: React.ReactNode; scale?: number; vTop?: number }> = ({ v, children, scale = 1, vTop = 640 }) => {
  const { width, height } = useVideoConfig();
  const p = useAppear(4, 18);
  const box = v ? { left: 0, top: vTop, width, height: height - vTop - 120 } : { left: 860, top: 0, width: width - 860, height };
  return (
    <div style={{ position: "absolute", ...box, display: "flex", alignItems: "center", justifyContent: "center" }}>
      <div style={{ transform: `translateY(${(1 - p) * 60}px) scale(${scale})`, opacity: p, position: "relative" }}>{children}</div>
    </div>
  );
};

// ---------------------------------------------------------------- scenes

const Hook: React.FC<{ t: any; v: boolean }> = ({ t, v }) => {
  const f = useCurrentFrame();
  const { width, height } = useVideoConfig();
  const swing = 14 * Math.exp(-f / 30) * Math.sin(f / 3.2);
  return (
    <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", textAlign: "center", padding: 80 }}>
      <Rings at={[4]} x={width / 2} y={height / 2 - (v ? 330 : 230)} max={v ? 700 : 800} />
      <div style={{ marginBottom: 50, opacity: interpolate(f, [0, 10], [0, 1], clamp) }}><Bell size={v ? 170 : 150} swing={swing} /></div>
      <Rise text={t.hook1} start={10} size={v ? 86 : 92} style={{ display: "flex", flexWrap: "wrap", justifyContent: "center" }} />
      <Rise text={t.hook2} start={42} size={v ? 86 : 92} gradient style={{ marginTop: 14, display: "flex", flexWrap: "wrap", justifyContent: "center" }} />
    </AbsoluteFill>
  );
};

const Pain: React.FC<{ t: any; v: boolean }> = ({ t, v }) => {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  return (
    <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", gap: v ? 34 : 26, flexDirection: "column" }}>
      <div style={{ display: "flex", flexWrap: "wrap", gap: v ? 26 : 30, justifyContent: "center", maxWidth: v ? 900 : 1500, flexDirection: v ? "column" : "row", alignItems: "center" }}>
        {t.pains.map((p: string, i: number) => {
          const a = spring({ frame: f - 6 - i * 10, fps, config: { damping: 9, mass: 0.6 } });
          const strike = interpolate(f, [62 + i * 4, 74 + i * 4], [0, 1], clamp);
          const shake = Math.sin((f - i * 10) * 1.7) * 5 * Math.max(0, 1 - (f - 6 - i * 10) / 14);
          return (
            <div key={i} style={{ position: "relative", display: "flex", alignItems: "center", gap: 16, padding: v ? "26px 40px" : "24px 36px", borderRadius: 22,
              background: "rgba(255,111,111,0.09)", border: "1px solid rgba(255,111,111,0.35)", fontFamily: inter, fontSize: v ? 46 : 40, fontWeight: 650, color: "#ffc9c9",
              transform: `scale(${a}) translateX(${shake}px)`, opacity: interpolate(f, [70 + i * 4, 90], [1, 0.35], clamp) }}>
              <Icon d={ICONS.x} size={v ? 44 : 38} color={C.bad} />
              {p}
              <div style={{ position: "absolute", left: 20, right: 20, top: "50%", height: 5, borderRadius: 4, background: C.bad, transformOrigin: "left", transform: `scaleX(${strike})` }} />
            </div>
          );
        })}
      </div>
      <Rise text={t.enough} start={84} size={v ? 96 : 88} gradient style={{ marginTop: v ? 40 : 46 }} />
    </AbsoluteFill>
  );
};

const Logo: React.FC<{ t: any; v: boolean }> = ({ t, v }) => {
  const f = useCurrentFrame();
  const { fps, width, height } = useVideoConfig();
  const drop = spring({ frame: f, fps, config: { damping: 11, mass: 0.9 } });
  const swing = 22 * Math.exp(-f / 22) * Math.sin(f / 2.6);
  const word = spring({ frame: f - 12, fps, config: { damping: 14 } });
  return (
    <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", flexDirection: "column", textAlign: "center" }}>
      <Rings at={[6, 30]} x={width / 2} y={height / 2 - (v ? 170 : 150)} max={v ? 900 : 1100} />
      <div style={{ transform: `translateY(${(1 - drop) * -260}px)` }}><Bell size={v ? 320 : 270} swing={swing} /></div>
      <div style={{ fontFamily: inter, fontWeight: 800, fontSize: v ? 190 : 170, letterSpacing: "-0.03em", lineHeight: 1, marginTop: 20,
        transform: `scale(${0.8 + word * 0.2})`, opacity: word,
        background: "linear-gradient(180deg,#fff3dc,#f0b35a)", WebkitBackgroundClip: "text", backgroundClip: "text", color: "transparent",
        filter: "drop-shadow(0 8px 40px rgba(240,168,74,0.35))" }}>Belfry</div>
      <Rise text={t.tagline} start={30} size={v ? 48 : 44} weight={500} color={C.muted} stagger={2}
        style={{ marginTop: 28, maxWidth: v ? 900 : 1400, display: "flex", flexWrap: "wrap", justifyContent: "center" }} />
    </AbsoluteFill>
  );
};

const Create: React.FC<{ t: any; v: boolean }> = ({ t, v }) => {
  const f = useCurrentFrame();
  const W = 760, H = 860;
  const name = typed(t.roomValue, 26, 16, f);
  const phase2 = f >= 84;
  const pc = press(f, 76), pcopy = press(f, 140);
  const codeShown = typed(CODE, 92, 160, f);
  const toast = interpolate(f, [142, 150, 190, 200], [0, 1, 1, 0], clamp);
  const dlg = useAppear(10);
  return (
    <>
      <Step n={1} title={t.s1} sub={t.s1sub} v={v} />
      <Stage v={v} scale={v ? 1.26 : 1} vTop={560}>
        <AppWindow w={W} h={H}>
          <div style={{ textAlign: "center", paddingTop: 70, opacity: 0.25 }}>
            <Bell size={130} />
            <div style={{ fontSize: 34, fontWeight: 700, marginTop: 20 }}>{t.hook2}</div>
          </div>
        </AppWindow>
        <div style={{ position: "absolute", inset: 0, borderRadius: 26, background: "rgba(6,4,8,0.6)", opacity: dlg }} />
        <div style={{ position: "absolute", left: 50, right: 50, top: 210, transform: `translateY(${(1 - dlg) * 30}px)`, opacity: dlg }}>
          <div style={{ background: C.card, border: `1px solid ${C.line2}`, borderRadius: 24, padding: "30px 30px 24px", boxShadow: "0 30px 80px rgba(0,0,0,0.6)", fontFamily: inter }}>
            {!phase2 ? (
              <>
                <div style={{ fontSize: 30, fontWeight: 700, marginBottom: 22 }}>{t.createTitle}</div>
                <div style={{ fontSize: 17, color: C.muted, fontWeight: 600, marginBottom: 8 }}>{t.roomName}</div>
                <div style={{ height: 62, borderRadius: 14, border: "1px solid rgba(240,168,74,0.6)", boxShadow: "0 0 0 4px rgba(240,168,74,0.14)",
                  background: "rgba(0,0,0,0.28)", display: "flex", alignItems: "center", padding: "0 18px", fontSize: 25 }}>{name}<Caret /></div>
                <div style={{ display: "flex", justifyContent: "flex-end", marginTop: 26 }}><Btn text={t.create} primary pressed={pc} /></div>
              </>
            ) : (
              <>
                <div style={{ fontSize: 30, fontWeight: 700, marginBottom: 18 }}>{t.shareTitle}</div>
                <div style={{ fontFamily: mono, fontSize: 21, lineHeight: 1.55, wordBreak: "break-all", color: C.accent2, background: "rgba(0,0,0,0.3)",
                  border: "1px dashed rgba(240,168,74,0.5)", borderRadius: 16, padding: "16px 18px", minHeight: 150 }}>{codeShown}</div>
                <div style={{ display: "flex", justifyContent: "flex-end", marginTop: 22 }}><Btn text={t.copy} primary pressed={pcopy} /></div>
              </>
            )}
          </div>
        </div>
        <div style={{ position: "absolute", left: "50%", bottom: 70, transform: `translate(-50%, ${(1 - toast) * 16}px)`, opacity: toast, background: C.card2,
          border: `1px solid ${C.line2}`, borderRadius: 16, padding: "14px 24px", fontFamily: inter, fontSize: 22, boxShadow: "0 12px 30px rgba(0,0,0,0.5)" }}>✓ {t.copied}</div>
        <Cursor path={[{ f: 60, x: 420, y: 560 }, { f: 74, x: 580, y: 450 }, { f: 120, x: 590, y: 470 }, { f: 136, x: 580, y: 610 }, { f: 170, x: 600, y: 640 }]} />
      </Stage>
    </>
  );
};

const Device: React.FC<{ x: number; y: number; name: string; os: string; icon: string; at: number; ok?: number; host?: boolean }> = ({ x, y, name, os, icon, at, ok = 0, host }) => {
  const p = useAppear(at);
  return (
    <div style={{ position: "absolute", left: x - 130, top: y - 105, width: 260, textAlign: "center", fontFamily: inter, transform: `scale(${p})`, opacity: p }}>
      <div style={{ width: 150, height: 150, margin: "0 auto", borderRadius: 40, display: "grid", placeItems: "center", position: "relative",
        background: host ? "linear-gradient(180deg, rgba(240,168,74,0.25), rgba(240,168,74,0.08))" : C.card2,
        border: `2px solid ${host ? "rgba(240,168,74,0.55)" : ok ? "rgba(95,211,154,0.6)" : C.line2}`, boxShadow: "0 18px 40px rgba(0,0,0,0.45)" }}>
        <Icon d={icon} size={82} color={host ? C.accent2 : C.text} />
        <div style={{ position: "absolute", right: -10, top: -10, width: 46, height: 46, borderRadius: 99, background: C.good, color: "#0b2416", fontWeight: 800,
          fontSize: 28, display: "grid", placeItems: "center", transform: `scale(${ok})` }}>✓</div>
      </div>
      <div style={{ fontSize: 28, fontWeight: 700, marginTop: 16 }}>{name}</div>
      <div style={{ fontSize: 19, color: C.muted }}>{os}</div>
    </div>
  );
};

const Share: React.FC<{ t: any; v: boolean }> = ({ t, v }) => {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  // positions inside a 980x720 (H) / 980x1000 (V) box
  const host = v ? { x: 490, y: 150 } : { x: 150, y: 360 };
  const friends = v
    ? [{ x: 230, y: 720, n: "Maria", os: "Windows 11", ic: ICONS.laptop, ms: 18 }, { x: 750, y: 720, n: "Gabriel", os: "Windows 10", ic: ICONS.monitor, ms: 31 }]
    : [{ x: 830, y: 130, n: "Maria", os: "Windows 11", ic: ICONS.laptop, ms: 18 }, { x: 830, y: 590, n: "Gabriel", os: "Windows 10", ic: ICONS.monitor, ms: 31 }];
  const box = v ? { w: 980, h: 960 } : { w: 980, h: 720 };
  return (
    <>
      <Step n={2} title={t.s2} sub={t.s2sub} v={v} />
      <Stage v={v} vTop={560} scale={v ? 1.08 : 1.0}>
        <div style={{ width: box.w, height: box.h, position: "relative" }}>
          <svg width={box.w} height={box.h} style={{ position: "absolute", inset: 0 }}>
            {friends.map((fr, i) => {
              const draw = interpolate(f, [80 + i * 12, 112 + i * 12], [0, 1], clamp);
              const hx = v ? host.x : host.x + 90, hy = v ? host.y + 150 : host.y; // leave the host's label clear
              const len = Math.hypot(fr.x - hx, fr.y - hy);
              return (
                <g key={i}>
                  <line x1={hx} y1={hy} x2={fr.x} y2={fr.y} stroke={C.good} strokeWidth={5} strokeLinecap="round"
                    strokeDasharray={`${len}`} strokeDashoffset={len * (1 - draw)} opacity={0.85} />
                  <line x1={hx} y1={hy} x2={fr.x} y2={fr.y} stroke={C.good} strokeWidth={16} opacity={0.12 * draw} />
                </g>
              );
            })}
          </svg>
          {friends.map((fr, i) => {
            // the code travels from the host to the friend
            const p = interpolate(f, [28 + i * 14, 66 + i * 14], [0, 1], { ...clamp, easing: Easing.inOut(Easing.cubic) });
            const vis = p > 0 && p < 1 ? 1 : 0;
            const x = host.x + (fr.x - host.x) * p, y = host.y + (fr.y - host.y) * p - Math.sin(p * Math.PI) * 90;
            const lbl = spring({ frame: f - 112 - i * 12, fps, config: { damping: 14 } });
            const mx = (host.x + fr.x) / 2, my = (host.y + fr.y) / 2;
            return (
              <React.Fragment key={i}>
                <div style={{ position: "absolute", left: x - 74, top: y - 26, width: 148, height: 52, borderRadius: 14, opacity: vis, fontFamily: mono, fontSize: 22,
                  fontWeight: 600, color: C.ink, background: `linear-gradient(180deg, ${C.accent2}, ${C.accent})`, display: "grid", placeItems: "center",
                  boxShadow: "0 10px 30px rgba(240,168,74,0.5)" }}>BLF39P…</div>
                <div style={{ position: "absolute", left: mx - 140, top: my - 26, width: 280, textAlign: "center", transform: `scale(${lbl})`, opacity: lbl }}>
                  <span style={{ background: "rgba(14,12,16,0.92)", border: "1px solid rgba(95,211,154,0.4)", borderRadius: 12, padding: "8px 14px", fontFamily: inter,
                    fontSize: 20, color: C.good, whiteSpace: "nowrap" }}>{t.direct} · IPv6 · <span style={{ fontFamily: mono }}>{fr.ms} ms</span></span>
                </div>
              </React.Fragment>
            );
          })}
          <Device x={host.x} y={host.y} name="Luis" os="Linux · Host" icon={ICONS.monitor} at={6} host />
          {friends.map((fr, i) => (
            <Device key={i} x={fr.x} y={fr.y} name={fr.n} os={fr.os} icon={fr.ic} at={14 + i * 6}
              ok={spring({ frame: f - 112 - i * 12, fps, config: { damping: 10 } })} />
          ))}
        </div>
      </Stage>
    </>
  );
};

const Play: React.FC<{ t: any; v: boolean }> = ({ t, v }) => {
  const f = useCurrentFrame();
  const members: MemberT[] = [
    { name: "Luis", ip: HOST_IP, host: true, me: true, at: 10 },
    { name: "Maria", ip: "10.62.18.2", ms: 18 + Math.sin(f / 9) * 2, at: 18 },
    { name: "Gabriel", ip: "10.62.18.3", ms: 31 + Math.sin(f / 7) * 3, at: 26 },
  ];
  const ipTxt = typed(`${HOST_IP}:9307`, 58, 14, f);
  const pc = press(f, 112);
  const ok = interpolate(f, [116, 126], [0, 1], clamp);
  const game = useAppear(40, 15);
  return (
    <>
      <Step n={3} title={t.s3} sub={t.s3sub} v={v} />
      <Stage v={v} scale={v ? 1.22 : 1.15} vTop={560}>
        <div style={{ width: 820, position: "relative", height: v ? 960 : 820 }}>
          <div style={{ position: "absolute", left: 0, top: v ? 0 : 40, width: 760 }}>
            <RoomCard name={t.roomValue} t={t} ip={HOST_IP} members={members} />
          </div>
          {/* a generic in-game "connect by IP" box */}
          <div style={{ position: "absolute", left: v ? 70 : 110, top: v ? 560 : 470, width: 680, transform: `translateY(${(1 - game) * 80}px) rotate(${(1 - game) * 3}deg)`,
            opacity: game, background: "linear-gradient(180deg, #2a1f1a, #16100d)", border: "2px solid #6b4a2c", borderRadius: 10, padding: "26px 30px",
            boxShadow: "0 30px 80px rgba(0,0,0,0.7)", fontFamily: "Georgia, 'Times New Roman', serif", color: "#e8d9c0" }}>
            <div style={{ fontSize: 30, letterSpacing: "0.14em", textTransform: "uppercase", color: "#d8b77a", marginBottom: 18 }}>{t.connectByIp}</div>
            <div style={{ height: 64, border: "1px solid #8a6a44", background: "rgba(0,0,0,0.45)", display: "flex", alignItems: "center", padding: "0 18px",
              fontFamily: mono, fontSize: 30, color: "#fff2d8" }}>{ipTxt}<Caret /></div>
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginTop: 20 }}>
              <span style={{ fontFamily: inter, fontSize: 22, color: C.good, opacity: ok, fontWeight: 600 }}>✓ {t.connected}</span>
              <div style={{ border: "1px solid #c59a5a", padding: "12px 30px", fontSize: 24, letterSpacing: "0.2em", transform: `scale(${1 - pc * 0.07})`,
                background: pc > 0 ? "rgba(216,183,122,0.25)" : "transparent" }}>{t.connect}</div>
            </div>
          </div>
        </div>
      </Stage>
    </>
  );
};

const Bubble: React.FC<{ who?: string; mine?: boolean; at: number; children: React.ReactNode; hue?: number }> = ({ who, mine, at, children, hue = 200 }) => {
  const p = useAppear(at, 14);
  return (
    <div style={{ display: "flex", flexDirection: "column", alignItems: mine ? "flex-end" : "flex-start", marginTop: 14, opacity: p, transform: `translateY(${(1 - p) * 20}px) scale(${0.96 + p * 0.04})` }}>
      {who && <span style={{ fontSize: 19, fontWeight: 650, color: `hsl(${hue} 75% 75%)`, margin: "0 0 4px 12px" }}>{who}</span>}
      <div style={{ maxWidth: 600, padding: "14px 20px 12px", fontSize: 28, borderRadius: mine ? "20px 20px 6px 20px" : "20px 20px 20px 6px",
        background: mine ? "linear-gradient(180deg, rgba(240,168,74,0.24), rgba(240,168,74,0.14))" : C.card2,
        border: `1px solid ${mine ? "rgba(240,168,74,0.32)" : C.line}` }}>{children}</div>
    </div>
  );
};

const Chat: React.FC<{ t: any; v: boolean }> = ({ t, v }) => {
  const f = useCurrentFrame();
  const prog = interpolate(f, [104, 150], [0, 1], { ...clamp, easing: Easing.inOut(Easing.quad) });
  const done = f >= 152;
  const drag = interpolate(f, [62, 84], [0, 1], { ...clamp, easing: Easing.inOut(Easing.cubic) });
  const dragVis = f >= 60 && f < 86 ? 1 : 0;
  return (
    <>
      <Step n={4} title={t.s4} sub={t.s4sub} v={v} />
      <Stage v={v} scale={v ? 1.26 : 1.08} vTop={560}>
        <AppWindow w={760} h={860}>
          <div style={{ borderBottom: `1px solid ${C.line}`, padding: "4px 6px 16px", display: "flex", alignItems: "center", gap: 14 }}>
            <span style={{ fontSize: 30, color: C.muted }}>←</span>
            <div><div style={{ fontSize: 24, fontWeight: 650 }}>{t.roomValue}</div><div style={{ fontSize: 16, color: C.muted }}>3 online</div></div>
          </div>
          <div style={{ padding: "6px 8px" }}>
            <Bubble who="Maria" at={10} hue={300}>{t.msgs[0]}</Bubble>
            <Bubble mine at={30}>{t.msgs[1]}</Bubble>
            <Bubble who="Gabriel" at={48} hue={150}>{t.msgs[2]}</Bubble>
            <Bubble who="Gabriel" at={86} hue={150}>
              <div style={{ display: "flex", alignItems: "center", gap: 16, minWidth: 470 }}>
                <Icon d={ICONS.file} size={50} />
                <div style={{ flex: 1 }}>
                  <div style={{ fontWeight: 650, fontSize: 24 }}>bloodborne_save_NG+.zip</div>
                  <div style={{ fontSize: 17, color: C.muted }}>{(48.2 * prog).toFixed(1)} / 48.2 MB</div>
                </div>
                {done ? (
                  <span style={{ color: C.good, fontWeight: 650, fontSize: 19 }}>✓ {t.downloaded}</span>
                ) : f >= 100 ? (
                  <div style={{ width: 120, height: 10, borderRadius: 99, background: "rgba(0,0,0,0.4)", overflow: "hidden" }}>
                    <div style={{ width: `${prog * 100}%`, height: "100%", background: C.accent2 }} />
                  </div>
                ) : (
                  <Btn text={t.download} primary pressed={press(f, 100)} style={{ height: 44, fontSize: 18, padding: "0 18px" }} />
                )}
              </div>
            </Bubble>
          </div>
          <div style={{ position: "absolute", left: 0, right: 0, bottom: 0, padding: "16px 22px 22px", borderTop: `1px solid ${C.line}`, background: C.bg, display: "flex", gap: 14, alignItems: "center" }}>
            <span style={{ fontSize: 28 }}>📎</span>
            <div style={{ flex: 1, height: 56, borderRadius: 16, border: `1px solid ${C.line2}`, background: "rgba(0,0,0,0.28)", display: "flex", alignItems: "center", padding: "0 18px", color: C.faint, fontSize: 21 }}>{t.typing}</div>
          </div>
        </AppWindow>
        {/* file dragged into the window */}
        <div style={{ position: "absolute", left: 820 - drag * 420, top: 120 + drag * 360, opacity: dragVis, transform: `rotate(${(1 - drag) * -12}deg)` }}>
          <div style={{ width: 110, height: 130, borderRadius: 16, background: C.card2, border: `2px solid ${C.accent}`, display: "grid", placeItems: "center", boxShadow: "0 20px 50px rgba(0,0,0,0.6)" }}>
            <Icon d={ICONS.file} size={70} />
          </div>
        </div>
        <Cursor path={[{ f: 92, x: 700, y: 640 }, { f: 100, x: 652, y: 612 }, { f: 130, x: 660, y: 640 }]} />
      </Stage>
    </>
  );
};

const Feats: React.FC<{ t: any; v: boolean }> = ({ t, v }) => {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const icons = [ICONS.user, ICONS.users, ICONS.lock, ICONS.monitor, ICONS.code, ICONS.refresh];
  return (
    <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", flexDirection: "column" }}>
      <Rise text={t.featTitle} start={2} size={v ? 80 : 76} gradient style={{ marginBottom: v ? 70 : 60, display: "flex", flexWrap: "wrap", justifyContent: "center", textAlign: "center" }} />
      <div style={{ display: "grid", gridTemplateColumns: v ? "repeat(2, 420px)" : "repeat(3, 440px)", gap: v ? 30 : 32 }}>
        {t.feats.map((x: string, i: number) => {
          const p = spring({ frame: f - 14 - i * 6, fps, config: { damping: 13 } });
          return (
            <div key={i} style={{ height: v ? 300 : 210, borderRadius: 28, background: `linear-gradient(180deg, ${C.card}, ${C.bg2})`, border: `1px solid ${C.line2}`,
              display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 20, fontFamily: inter, fontSize: v ? 36 : 32, fontWeight: 650,
              textAlign: "center", padding: 20, transform: `translateY(${(1 - p) * 50}px) scale(${0.9 + p * 0.1})`, opacity: p, boxShadow: "0 18px 40px rgba(0,0,0,0.3)" }}>
              <div style={{ width: 84, height: 84, borderRadius: 24, display: "grid", placeItems: "center", background: "rgba(240,168,74,0.14)" }}>
                <Icon d={icons[i]} size={50} />
              </div>
              {x}
            </div>
          );
        })}
      </div>
    </AbsoluteFill>
  );
};

const End: React.FC<{ t: any; v: boolean }> = ({ t, v }) => {
  const f = useCurrentFrame();
  const { fps, width, height } = useVideoConfig();
  const swing = 18 * Math.exp(-f / 26) * Math.sin(f / 2.8);
  const p = spring({ frame: f - 4, fps, config: { damping: 13 } });
  const url = useAppear(40);
  return (
    <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", flexDirection: "column", textAlign: "center" }}>
      <Rings at={[4, 40]} x={width / 2} y={height / 2 - (v ? 300 : 250)} max={v ? 900 : 1100} />
      <div style={{ transform: `scale(${p})` }}><Bell size={v ? 240 : 200} swing={swing} /></div>
      <div style={{ fontFamily: inter, fontWeight: 800, fontSize: v ? 150 : 130, letterSpacing: "-0.03em", lineHeight: 1, marginTop: 10, opacity: p,
        background: "linear-gradient(180deg,#fff3dc,#f0b35a)", WebkitBackgroundClip: "text", backgroundClip: "text", color: "transparent" }}>Belfry</div>
      <Rise text={t.endLine} start={18} size={v ? 54 : 50} weight={650} style={{ marginTop: 24, display: "flex", flexWrap: "wrap", justifyContent: "center", maxWidth: v ? 900 : 1500 }} />
      <div style={{ marginTop: 44, opacity: url, transform: `translateY(${(1 - url) * 20}px)`, fontFamily: mono, fontSize: v ? 34 : 34, color: C.accent2,
        background: "rgba(0,0,0,0.35)", border: "1px solid rgba(240,168,74,0.4)", borderRadius: 18, padding: "16px 30px" }}>github.com/ludoviciphilip/belfry</div>
      <div style={{ marginTop: 26, fontFamily: inter, fontSize: v ? 30 : 28, color: C.muted, opacity: url }}>{t.endSub}</div>
    </AbsoluteFill>
  );
};

export const Trailer: React.FC<{ lang: Lang }> = ({ lang }) => {
  const { width, height } = useVideoConfig();
  const v = height > width;
  const t = S[lang];
  return (
    <AbsoluteFill style={{ fontFamily: inter, color: C.text }}>
      <Backdrop />
      <Scene k="hook"><Hook t={t} v={v} /></Scene>
      <Scene k="pain"><Pain t={t} v={v} /></Scene>
      <Scene k="logo"><Logo t={t} v={v} /></Scene>
      <Scene k="create"><Create t={t} v={v} /></Scene>
      <Scene k="share"><Share t={t} v={v} /></Scene>
      <Scene k="play"><Play t={t} v={v} /></Scene>
      <Scene k="chat"><Chat t={t} v={v} /></Scene>
      <Scene k="feats"><Feats t={t} v={v} /></Scene>
      <Scene k="end" fadeOut={false}><End t={t} v={v} /></Scene>
    </AbsoluteFill>
  );
};
