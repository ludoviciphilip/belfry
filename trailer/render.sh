#!/usr/bin/env bash
# Renders the four trailers (horizontal/vertical x pt/en) into out/.
set -euo pipefail
cd "$(dirname "$0")"
/usr/bin/python3 music.py
ffmpeg -hide_banner -loglevel error -y -i public/music_raw.wav -af loudnorm=I=-14:TP=-1.5:LRA=11 -ar 48000 public/music.wav
rm -f public/music_raw.wav
mkdir -p out
for comp in Horizontal-pt Vertical-pt Horizontal-en Vertical-en; do
  /usr/bin/node node_modules/@remotion/cli/remotion-cli.js render src/index.ts "$comp" "out/belfry-trailer-${comp,,}.mp4" \
    --codec=h264 --crf=16 --audio-bitrate=320k --concurrency=8 --log=error
  echo "done $comp"
done
