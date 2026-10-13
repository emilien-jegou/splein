import ViewMacro from "./components/sections/01ViewMacro";
import Group from "./components/sections/02Group";
import Text from "./components/sections/03Text";
import MarginPadding from "./components/sections/04MarginPadding";
import FillStrokeRadius from "./components/sections/05FillStrokeRadius";
import Anchors from "./components/sections/06Anchors";
import Alignment from "./components/sections/07Alignment";
import Scroll from "./components/sections/08Scroll";
import Signals from "./components/sections/09Signals";
import Transitions from "./components/sections/10Transitions";
import Transform from "./components/sections/11Transform";
import Opacity from "./components/sections/12Opacity";
import Playback from "./components/sections/13Playback";
import Springs from "./components/sections/14Springs";
import PressFeedback from "./components/sections/15PressFeedback";
import DragReorder from "./components/sections/16DragReorder";
import FrameClock from "./components/sections/17FrameClock";
import Shadows from "./components/sections/18Shadows";
import TextLayout from "./components/sections/19TextLayout";
import TextWrap from "./components/sections/20TextWrap";
import AutoGrid from "./components/sections/21AutoGrid";
import SelectableText from "./components/sections/22SelectableText";
import VectorGraphic from "./components/sections/23VectorGraphic";
import Overlay from "./components/sections/24Overlay";
import Context from "./components/sections/25Context";
import HitTest from "./components/sections/26HitTest";
import LightEnvironment from "./components/sections/27LightEnvironment";
import AppExtension from "./components/sections/28AppExtension";
import HeadlessEngine from "./components/sections/29HeadlessEngine";
import HotReload from "./components/sections/30HotReload";
import KeyedFor from "./components/sections/31KeyedFor";
import ElementRef from "./components/sections/32ElementRef";
import DynamicProp from "./components/sections/33DynamicProp";
import Visible from "./components/sections/34Visible";
import FontFamily from "./components/sections/35FontFamily";

export default function Home() {
  return (
    <main className="min-h-screen bg-white">
      <ViewMacro />
      <Group />
      <Text />
      <MarginPadding />
      <FillStrokeRadius />
      <Anchors />
      <Alignment />
      <Scroll />
      <Signals />
      <Transitions />
      <Transform />
      <Opacity />
      <Playback />
      <Springs />
      <PressFeedback />
      <DragReorder />
      <FrameClock />
      <Shadows />
      <TextLayout />
      <TextWrap />
      <AutoGrid />
      <SelectableText />
      <VectorGraphic />
      <Overlay />
      <Context />
      <HitTest />
      <LightEnvironment />
      <AppExtension />
      <HeadlessEngine />
      <HotReload />
      <KeyedFor />
      <ElementRef />
      <DynamicProp />
      <Visible />
      <FontFamily />
    </main>
  );
}
