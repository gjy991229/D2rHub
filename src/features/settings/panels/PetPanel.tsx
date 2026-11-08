import { useState } from "react";
import { PetProgressPanel } from "../../pet/PetProgressPanel";
import { PetWardrobePanel } from "../../pet/PetWardrobePanel";
import { LocateFixed } from "lucide-react";
import { Button } from "../../../components/ui/Button";
import { RangeSlider } from "../../../components/ui/RangeSlider";
import { Toggle } from "../../../components/ui/Toggle";
import type { GlobalConfig } from "../../../store/types";
import "../../pet/petUi.css";

interface PetPanelProps {
  config: GlobalConfig;
  windowPlacementBusy: string | null;
  updateConfig: (updater: (config: GlobalConfig) => void) => void;
  persistConfig: (draft: GlobalConfig, quiet?: boolean) => Promise<unknown>;
  onLocate: () => void | Promise<void>;
}
export function PetPanel({
  config,
  windowPlacementBusy,
  updateConfig,
  persistConfig,
  onLocate,
}: PetPanelProps) {
  const [tab, setTab] = useState("wardrobe");
  const english = config.app_language === "en-US";
  const tabs = [["wardrobe", english ? "Wardrobe" : "装扮衣柜"], ["progress", english ? "Achievements & stats" : "成就与统计"], ["settings", english ? "Behavior" : "行为设置"]];
  return (
    <div className="pet-workspace space-y-4" data-i18n-skip="true">
      <nav className="settings-section-tabs flex flex-wrap" aria-label={english ? "Pet sections" : "小猫设置分页"}>{tabs.map(([id, label]) => <button key={id} type="button" aria-current={tab === id ? "page" : undefined} onClick={() => setTab(id)} className={`rounded-md px-4 py-2 text-sm font-semibold transition-colors ${tab === id ? "bg-surface-active text-accent" : "text-text-muted hover:bg-surface-hover"}`}>{label}</button>)}</nav>
      {tab === "wardrobe" && <PetWardrobePanel english={english} />}
      {tab === "progress" && <PetProgressPanel english={english} />}
      {tab === "settings" && (
      <section className="spatial-panel p-3 space-y-2" aria-labelledby="pet-module-title">
        <div className="flex items-center justify-between gap-3 py-1">
          <div>
            <h2 id="pet-module-title" className="text-sm font-semibold text-text-primary">{english ? "Desktop companion" : "桌面伴随"}</h2>
            <p className="text-xs text-text-muted">{english ? "React to keyboard and mouse activity with optional status bubbles." : "随键鼠活动做出动作，也可显示状态气泡。"}</p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Button
              variant="ghost"
              size="sm"
              loading={windowPlacementBusy === "bongo-cat"}
              disabled={!config.enable_bongo_cat || windowPlacementBusy !== null}
              onClick={onLocate}
              title={english ? "Move to the same display as D2RHub" : "将桌宠移到主界面所在屏幕"}
            >
              <LocateFixed size={12} />
              {english ? "Locate" : "定位"}
            </Button>
            <Toggle
              checked={!!config.enable_bongo_cat}
              ariaLabel={english ? "Show desktop pet" : "显示桌宠"}
              onChange={async enabled => {
                updateConfig(current => { current.enable_bongo_cat = enabled; });
                const current = config;
                if (current) await persistConfig({ ...current, enable_bongo_cat: enabled }, true);
              }}
            />
          </div>
        </div>

        {config.enable_bongo_cat && (
          <div className="space-y-3 border-t border-border-default/50 pt-3">
            <div className="flex items-center justify-between gap-3">
              <div>
                <span className="text-sm font-semibold text-text-primary">{english ? "Speech bubbles" : "气泡对话"}</span>
                <p className="text-xs text-text-muted">{english ? "Show occasional chatter and application status." : "偶尔显示俏皮台词和软件状态提示。"}</p>
              </div>
              <Toggle
                checked={!!config.bongo_cat_chatterbox}
                ariaLabel={english ? "Pet speech bubbles" : "桌宠气泡对话框"}
                onChange={enabled => updateConfig(current => { current.bongo_cat_chatterbox = enabled; })}
              />
            </div>

            <div className="space-y-1">
              <div className="flex justify-between items-center text-xs">
                <label htmlFor="pet-scale" className="font-semibold text-text-secondary">{english ? "Companion size" : "桌宠大小"}</label>
                <span className="font-mono text-accent font-bold">{(config.bongo_cat_scale ?? 1.0).toFixed(1)}x</span>
              </div>
              <RangeSlider
                id="pet-scale"
                min={5}
                max={50}
                value={Math.round((config.bongo_cat_scale ?? 1.0) * 10)}
                onChange={async (event) => {
                  const scale = parseFloat(event.target.value) / 10;
                  updateConfig(current => { current.bongo_cat_scale = scale; });
                  try {
                    const current = config;
                    if (current) await persistConfig({ ...current, bongo_cat_scale: scale }, true);
                  } catch (error) {
                    console.error("保存猫咪缩放失败", error);
                  }
                }}
                className="w-full"
              />
            </div>


          </div>
        )}
      </section>
      )}
    </div>
  );
}

