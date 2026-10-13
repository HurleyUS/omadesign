import {AbsoluteFill, OffthreadVideo, staticFile} from "remotion";

export const OmadesignVideo: React.FC = () => (
  <AbsoluteFill>
    <OffthreadVideo src={staticFile("media/hero.mp4")} muted />
  </AbsoluteFill>
);
