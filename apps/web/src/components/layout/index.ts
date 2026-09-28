// Layout Components barrel export

export { Header } from "./Header";
export type { HeaderProps } from "./Header";

export { Sidebar } from "./Sidebar";
export type { SidebarProps, SidebarPosition } from "./Sidebar";

export { StatusBar } from "./StatusBar";
export type { StatusBarProps } from "./StatusBar";

export { ProgressOverlay } from "./ProgressOverlay";
export type { ProgressOverlayProps } from "./ProgressOverlay";

export { Viewport } from "./Viewport";
export type { ViewportProps } from "./Viewport";

export { ContextPanel } from "./ContextPanel";
export type { ContextPanelProps } from "./ContextPanel";

export { WelcomeScreen } from "./WelcomeScreen";
export type { WelcomeScreenProps } from "./WelcomeScreen";

export { Toolbar } from "./Toolbar";
export type { ToolbarProps } from "./Toolbar";

export { Timeline } from "./Timeline";
export type { TimelineProps, TimelineRow } from "./Timeline";

// Re-export Viewer3D types for convenience
export type {
  Viewer3D,
  MeshData,
  SkeletonData,
  BoneData,
  WeightsData,
  ViewerSettings,
} from "../../lib/Viewer3D";
