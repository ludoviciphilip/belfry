#!/usr/bin/env python3
"""Soundtrack for the Belfry trailer: synthesized music + SFX synced to the
scene frames in src/Trailer.tsx. Writes public/music.wav (48 kHz stereo).
Run with /usr/bin/python3 (numpy)."""
import numpy as np
import wave

SR = 48000
FPS = 30
TOTAL = 1415 / FPS
N = int(TOTAL * SR) + SR
rng = np.random.default_rng(7)
L = np.zeros(N)
R = np.zeros(N)

def at(sec):
    return int(sec * SR)

def fr(frame):
    return frame / FPS

def add(sig, start, gain=1.0, pan=0.0):
    i = at(start)
    if i >= N:
        return
    sig = sig[: N - i]
    L[i:i + len(sig)] += sig * gain * np.sqrt(0.5 * (1 - pan))
    R[i:i + len(sig)] += sig * gain * np.sqrt(0.5 * (1 + pan))

def env(n, a, d, sustain=1.0, release=None):
    e = np.ones(n) * sustain
    na = max(1, int(a * SR))
    e[:na] = np.linspace(0, 1, na)
    if release:
        nr = min(n, int(release * SR))
        e[-nr:] *= np.linspace(1, 0, nr)
    return e

def lp_fast(x, cutoff):
    """Lowpass in the frequency domain (fine for short one-shots)."""
    X = np.fft.rfft(x)
    f = np.fft.rfftfreq(len(x), 1 / SR)
    H = 1 / np.sqrt(1 + (f / cutoff) ** 4)
    return np.fft.irfft(X * H, len(x))

def hz(note):  # MIDI -> Hz
    return 440.0 * 2 ** ((note - 69) / 12)

# ---------------------------------------------------------------- instruments

def bell(freq, dur=5.0):
    t = np.arange(int(dur * SR)) / SR
    partials = [(0.5, 0.35, 0.6), (1.0, 1.0, 1.0), (1.19, 0.5, 0.8), (1.56, 0.4, 0.65), (2.0, 0.55, 0.5),
                (2.51, 0.3, 0.4), (2.66, 0.25, 0.35), (3.01, 0.2, 0.3), (4.1, 0.12, 0.2)]
    s = np.zeros_like(t)
    for ratio, amp, life in partials:
        s += amp * np.sin(2 * np.pi * freq * ratio * t + rng.uniform(0, 6)) * np.exp(-t / (life * dur * 0.5))
    s *= np.minimum(1, t / 0.003)
    return s / 3

def pad(notes, dur):
    t = np.arange(int(dur * SR)) / SR
    s = np.zeros_like(t)
    for n in notes:
        for det in (-0.08, 0.0, 0.08):
            f = hz(n + det)
            # soft saw: sum of a few harmonics
            for k in range(1, 7):
                s += np.sin(2 * np.pi * f * k * t + k) / (k ** 1.6)
    s *= env(len(t), 0.6, 0, release=0.8)
    return s / (len(notes) * 6)

def pluck(freq, dur=0.5):
    t = np.arange(int(dur * SR)) / SR
    s = (np.sin(2 * np.pi * freq * t) + 0.35 * np.sin(2 * np.pi * 2 * freq * t) + 0.12 * np.sin(2 * np.pi * 3 * freq * t))
    return s * np.exp(-t * 9) * np.minimum(1, t / 0.002)

def kick():
    t = np.arange(int(0.45 * SR)) / SR
    f = 50 + 110 * np.exp(-t * 28)
    ph = 2 * np.pi * np.cumsum(f) / SR
    return np.sin(ph) * np.exp(-t * 7.5)

def hat():
    n = int(0.06 * SR)
    x = rng.standard_normal(n)
    x = np.diff(np.concatenate([[0], x]))  # crude highpass
    x = lp_fast(x, 9000)  # keep it soft, not hissy
    return x * np.exp(-np.arange(n) / SR * 70) * 0.5

def snare():
    n = int(0.25 * SR)
    t = np.arange(n) / SR
    x = rng.standard_normal(n) * np.exp(-t * 18) * 0.6 + np.sin(2 * np.pi * 190 * t) * np.exp(-t * 25) * 0.5
    return lp_fast(x, 6000)

def whoosh(dur=0.7, rising=True):
    n = int(dur * SR)
    t = np.arange(n) / SR
    x = rng.standard_normal(n)
    e = np.sin(np.pi * t / dur) ** 2
    # sweep: blend lowpassed and raw noise over time
    lo = lp_fast(x, 900)
    hi = lp_fast(x, 5000)
    k = t / dur if rising else 1 - t / dur
    return (lo * (1 - k) + hi * k) * e * 0.5

def click(f0=2200):
    n = int(0.03 * SR)
    t = np.arange(n) / SR
    return np.sin(2 * np.pi * f0 * t) * np.exp(-t * 260) * 0.7 + rng.standard_normal(n) * np.exp(-t * 400) * 0.15

def pop(freq=880):
    n = int(0.16 * SR)
    t = np.arange(n) / SR
    f = freq * (1 + 0.6 * np.exp(-t * 40))
    return np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t * 22) * 0.6

def chime(notes, step=0.07):
    out = np.zeros(int((len(notes) * step + 0.8) * SR))
    for i, n in enumerate(notes):
        s = pluck(hz(n), 0.8) * 0.8
        o = int(i * step * SR)
        out[o:o + len(s)] += s
    return out

def drone(freq, dur):
    t = np.arange(int(dur * SR)) / SR
    s = np.sin(2 * np.pi * freq * t) + 0.5 * np.sin(2 * np.pi * freq * 1.5 * t + 1) + 0.3 * np.sin(2 * np.pi * freq * 2.03 * t)
    trem = 0.75 + 0.25 * np.sin(2 * np.pi * 3 * t)
    return s * trem * env(len(t), 0.4, 0, release=0.5) / 2

# ---------------------------------------------------------------- arrangement
BPM = 100
BEAT = 60 / BPM
BAR = 4 * BEAT
LOGO = fr(225)
END = fr(1290)
# A minor: Am, F, C, G (pad voicings, MIDI)
CHORDS = [[57, 60, 64], [53, 57, 60], [55, 60, 64], [55, 59, 62]]
ARP = [[69, 72, 76, 72], [65, 69, 72, 69], [67, 72, 76, 72], [67, 71, 74, 71]]

# 1. hook: bell + soft pad
add(bell(hz(69)), 0.15, 0.55)
add(pad(CHORDS[0], fr(105) + 0.8), 0.0, 0.22)

# 2. pain: low tension drone + tick-tock
add(drone(hz(45), fr(120) + 0.4), fr(105), 0.20)
for i in range(4):
    add(pop(300), fr(105 + 6 + i * 10), 0.35, pan=-0.3 + i * 0.2)
for i in range(4):
    add(whoosh(0.25, rising=False), fr(105 + 62 + i * 4), 0.25)
add(whoosh(1.0, rising=True), LOGO - 1.0, 0.55)  # riser into the logo

# 3. logo: big bell + music starts
add(bell(hz(57), 6.0), LOGO + 0.03, 0.85)
add(bell(hz(69), 5.0), LOGO + fr(30), 0.35, pan=0.2)

music_start = LOGO
bars = int((END - music_start) / BAR) + 1
for b in range(bars):
    t0 = music_start + b * BAR
    ch = b % 4
    add(pad(CHORDS[ch], BAR + 0.8), t0, 0.26)
    add(pad([CHORDS[ch][0] - 12], BAR + 0.8), t0, 0.16)  # bass
    groove = b >= 1  # drums after the logo has breathed
    for k in range(8):
        tk = t0 + k * BEAT / 2
        if tk >= END:
            break
        n = ARP[ch][k % 4] + (12 if k >= 4 and b % 2 else 0)
        add(pluck(hz(n)), tk, 0.18, pan=0.35 if k % 2 else -0.35)
        if groove:
            if k % 2 == 0:
                add(kick(), tk, 0.55)
            else:
                add(hat(), tk, 0.14, pan=0.2)
            if k in (2, 6):
                add(snare(), tk, 0.20)

# 4. end: bell + final chord
add(whoosh(0.9, True), END - 0.9, 0.4)
add(bell(hz(57), 6.0), END + fr(4), 0.85)
add(bell(hz(64), 5.0), END + fr(40), 0.30, pan=-0.2)
add(pad(CHORDS[0] + [69], TOTAL - END + 1), END, 0.30)

# ---------------------------------------------------------------- SFX on UI moments
for s in (105, 330, 540, 750, 930, 1110):
    add(whoosh(0.6, True), fr(s) - 0.35, 0.35)
# create: typing the room name, pressing Create, code appears, copy
name_len = 15
for i in range(name_len):
    add(click(1800 + rng.uniform(-300, 300)), fr(330 + 26) + i / 16, 0.30, pan=0.3)
add(click(1200), fr(330 + 76), 0.6)
add(chime([76, 81, 84]), fr(330 + 90), 0.30)
add(click(1200), fr(330 + 140), 0.6)
add(pop(1320), fr(330 + 144), 0.45)
# share: code flies, connections confirm
for i in range(2):
    add(whoosh(0.5, True), fr(540 + 28 + i * 14), 0.25, pan=0.4)
    add(chime([81, 88]), fr(540 + 112 + i * 12), 0.35, pan=0.3 * (1 if i else -1))
# play: members join, IP typed, connect
for i in range(3):
    add(pop(700 + i * 120), fr(750 + 10 + i * 8), 0.35)
for i in range(15):
    add(click(2000), fr(750 + 58) + i / 14, 0.25, pan=0.3)
add(click(1100), fr(750 + 112), 0.6)
add(chime([72, 76, 79, 84]), fr(750 + 116), 0.38)
# chat: bubbles, file drop, download done
for k, a in enumerate((10, 30, 48, 86)):
    add(pop(980 if k % 2 == 0 else 760), fr(930 + a), 0.45, pan=-0.2 if k % 2 == 0 else 0.25)
add(click(1100), fr(930 + 100), 0.5)
add(chime([79, 84, 88]), fr(930 + 152), 0.35)
# features: soft ticks
for i in range(6):
    add(pop(600 + i * 90), fr(1110 + 14 + i * 6), 0.25, pan=-0.4 + i * 0.16)

# ---------------------------------------------------------------- master
mix = np.stack([L, R], axis=1)
mix = mix[: int(TOTAL * SR)]
fade = int(1.2 * SR)
mix[-fade:] *= np.linspace(1, 0, fade)[:, None]
mix /= np.max(np.abs(mix)) + 1e-9
mix *= 0.89
pcm = (mix * 32767).astype(np.int16)
with wave.open("public/music_raw.wav", "wb") as w:
    w.setnchannels(2)
    w.setsampwidth(2)
    w.setframerate(SR)
    w.writeframes(pcm.tobytes())
print("public/music_raw.wav", round(TOTAL, 2), "s")
