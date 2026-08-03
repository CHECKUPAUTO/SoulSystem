import "./index.css";
import { Composition } from "remotion";
import {
  SoulSystemArchitecture,
  TOTAL_DURATION,
} from "./SoulSystemArchitecture";

export const RemotionRoot: React.FC = () => {
  return (
    <>
      <Composition
        id="SoulSystemArchitecture"
        component={SoulSystemArchitecture}
        durationInFrames={TOTAL_DURATION}
        fps={30}
        width={1280}
        height={720}
      />
    </>
  );
};
