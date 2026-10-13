import {Composition} from "remotion";
import {OmadesignVideo} from "./OmadesignVideo";

export const RemotionRoot = () => {
  return (
    <>
      <Composition
        id="Omadesign"
        component={OmadesignVideo}
        durationInFrames={1200}
        fps={30}
        width={1600}
        height={900}
        defaultProps={{}}
      />
    </>
  );
};
