// node stills.mjs <comp> <outdir> <frame,frame,...>  (music off)
import { bundle } from "@remotion/bundler";
import { renderStill, selectComposition } from "@remotion/renderer";
import path from "node:path";
const [comp, out, frames] = process.argv.slice(2);
const serveUrl = await bundle({ entryPoint: path.resolve("src/index.ts") });
const inputProps = { lang: comp.endsWith("en") ? "en" : "pt", music: false };
const composition = await selectComposition({ serveUrl, id: comp, inputProps });
for (const f of frames.split(",").map(Number)) {
  await renderStill({ composition, serveUrl, frame: f, output: `${out}/${comp}-${String(f).padStart(4, "0")}.png`, inputProps, scale: 0.5 });
  console.log("frame", f);
}
