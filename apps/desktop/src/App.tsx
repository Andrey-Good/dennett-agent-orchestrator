import { useMemo } from "react";

import {
  createTauriBridge,
  createWindowControls,
  isTauriRuntime,
  type DesktopBridge,
  type WindowControls,
} from "./api/bridge";
import { createBrowserPreviewBridge } from "./api/mockBridge";
import { ChatCard } from "./components/ChatCard";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { WorkspacePanel } from "./components/WorkspacePanel";
import { useWorkspace, WorkspaceProvider } from "./state/WorkspaceProvider";

export interface AppProps {
  bridge?: DesktopBridge;
  windowControls?: WindowControls;
  initialRightPanelOpen?: boolean;
}

function DesktopShell() {
  const { state } = useWorkspace();

  return (
    <div
      className={
        state.isSidebarOpen
          ? "app-shell"
          : "app-shell app-shell--sidebar-collapsed"
      }
      data-testid="desktop-app-shell"
    >
      <TitleBar />
      <Sidebar />
      <div className="workspace-stage" data-testid="workspace-stage">
        <ChatCard />
        <WorkspacePanel />
      </div>
    </div>
  );
}

export function App({ bridge, windowControls, initialRightPanelOpen = false }: AppProps) {
  const resolvedBridge = useMemo<DesktopBridge>(() => {
    if (bridge) {
      return bridge;
    }
    return isTauriRuntime() ? createTauriBridge() : createBrowserPreviewBridge();
  }, [bridge]);

  const resolvedWindowControls = useMemo<WindowControls>(
    () => windowControls ?? createWindowControls(),
    [windowControls],
  );

  return (
    <WorkspaceProvider
      bridge={resolvedBridge}
      windowControls={resolvedWindowControls}
      initialRightPanelOpen={initialRightPanelOpen}
    >
      <DesktopShell />
    </WorkspaceProvider>
  );
}

export default App;
