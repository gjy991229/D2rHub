import { useEffect, useRef } from "react";
import { listenEvent } from "../platform/tauri";
import { useGlobalConfig } from "../store/globalConfig";
import { useLaunch } from "../store/launch";
import type { LaunchGroup } from "../store/types";
import { showToast } from "../components/ui/Toast";

/** Global shortcuts share the dashboard's startup and operation guards. */
export function useLaunchSchemeShortcut(
  enabled: boolean,
  onLaunch: (group: LaunchGroup) => Promise<void>,
) {
  const latest = useRef({ enabled, onLaunch });
  latest.current = { enabled, onLaunch };
  const dispatching = useRef(false);

  useEffect(() => {
    let disposed = false;
    const listener = listenEvent<string>("launch-scheme-shortcut", event => {
      const settings = useGlobalConfig.getState();
      if (disposed || !latest.current.enabled || dispatching.current
        || settings.initialLoading || settings.saving || settings.restarting
        || useLaunch.getState().launching) return;
      const group = settings.config?.launch_groups.find(group => group.id === event.payload);
      if (!group) return;
      dispatching.current = true;
      void latest.current.onLaunch(group).catch(error => {
        showToast("error", settings.config?.app_language === "en-US"
          ? `Could not launch scheme: ${error}` : `启动方案失败：${error}`);
      }).finally(() => { dispatching.current = false; });
    });
    return () => {
      disposed = true;
      void listener.then(stop => stop()).catch(() => {});
    };
  }, []);
}
