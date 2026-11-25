import { useGlobalConfig } from "./globalConfig";

export function prefersReducedMotion(): boolean {
  return document.documentElement.dataset.motion === "reduced";
}

export function syncMotionPreference() {
  const apply = () => {
    document.documentElement.dataset.motion =
      useGlobalConfig.getState().config?.ui_motion === "reduced" ? "reduced" : "full";
  };
  apply();
  const unsubscribe = useGlobalConfig.subscribe((state, previous) => {
    if (state.config?.ui_motion !== previous.config?.ui_motion) apply();
  });
  if (import.meta.hot) import.meta.hot.dispose(unsubscribe);
}
