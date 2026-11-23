import { useMemo } from "react";

import {
  createTauriBridge,
  createWindowControls,
  isTauriRuntime,
  type DesktopBridge,
  type WindowControls,
} from "./api/bridge";
import { createDevHttpBridge } from "./api/devHttpBridge";
import { createBrowserPreviewBridge } from "./api/mockBridge";
import { ActivityRail } from "./components/ActivityRail";
import { ChatCard } from "./components/ChatCard";
import { CommandPalette } from "./components/CommandPalette";
import { DevFolderPickerModal } from "./components/DevFolderPickerModal";
import { SettingsSheet } from "./components/SettingsSheet";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { UserSettingsBar } from "./components/UserSettingsBar";
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
      className={state.isSidebarOpen ? "workbench" : "workbench sidebar-collapsed"}
      data-testid="desktop-app-shell"
    >
      <TitleBar />
      <ActivityRail />
      <Sidebar />
      <UserSettingsBar />
      <div className="workspace-stage" data-testid="workspace-stage">
        <ChatCard />
        <WorkspacePanel />
      </div>
      <SettingsSheet />
      <CommandPalette />
      {import.meta.env.DEV && <DevFolderPickerModal />}
    </div>
  );
}

export function App({ bridge, windowControls, initialRightPanelOpen = false }: AppProps) {
  const resolvedBridge = useMemo<DesktopBridge>(() => {
    if (bridge) {
      return bridge;
    }
    if (isTauriRuntime()) {
      return createTauriBridge();
    }
    if (
      import.meta.env.DEV &&
      typeof window !== "undefined" &&
      !new URLSearchParams(window.location.search).has("mock")
    ) {
      return createDevHttpBridge("/__dennett_dev_head");
    }
    return createBrowserPreviewBridge();
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
