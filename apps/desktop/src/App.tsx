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
import { InboxRadarCard } from "./components/InboxRadarCard";
import { MemoryCard } from "./components/MemoryCard";
import { SettingsSheet } from "./components/SettingsSheet";
import { Sidebar } from "./components/Sidebar";
import { TasksCard } from "./components/TasksCard";
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
  const { state, meta } = useWorkspace();

  return (
    <div
      className={meta.isSidebarEffectiveOpen ? "workbench" : "workbench sidebar-collapsed"}
      data-testid="desktop-app-shell"
      data-active-view={state.activeView}
    >
      <TitleBar />
      <ActivityRail />
      <Sidebar />
      <UserSettingsBar />
      <div className="workspace-stage" data-testid="workspace-stage">
        {state.activeView === "inbox" ? (
          <InboxRadarCard />
        ) : state.activeView === "tasks" ? (
          <TasksCard />
        ) : state.activeView === "memory" ? (
          <MemoryCard />
        ) : (
          <ChatCard />
        )}
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
