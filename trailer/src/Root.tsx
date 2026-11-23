import React from "react";
import { Composition, Audio, staticFile, AbsoluteFill } from "remotion";
import { Trailer, TOTAL } from "./Trailer";
import { Lang } from "./strings";

const WithMusic: React.FC<{ lang: Lang; music: boolean }> = ({ lang, music }) => (
  <AbsoluteFill>
    <Trailer lang={lang} />
    {music && <Audio src={staticFile("music.wav")} />}
  </AbsoluteFill>
);

export const Root: React.FC = () => (
  <>
    {(["pt", "en"] as const).flatMap((lang) => [
      <Composition key={`h-${lang}`} id={`Horizontal-${lang}`} component={WithMusic} durationInFrames={TOTAL} fps={30} width={1920} height={1080} defaultProps={{ lang, music: true }} />,
      <Composition key={`v-${lang}`} id={`Vertical-${lang}`} component={WithMusic} durationInFrames={TOTAL} fps={30} width={1080} height={1920} defaultProps={{ lang, music: true }} />,
    ])}
  </>
);
