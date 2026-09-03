import { LocateFixed } from "lucide-react";
import { Button } from "../../../components/ui/Button";
import { RangeSlider } from "../../../components/ui/RangeSlider";
import { Toggle } from "../../../components/ui/Toggle";
import { useGlobalConfig } from "../../../store/globalConfig";
import type { GlobalConfig } from "../../../store/types";

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
  return (
    <div className="settings-content-grid">
      <section className="spatial-panel p-3 space-y-2" aria-labelledby="pet-module-title">
        <div className="flex items-center justify-between gap-3 py-1">
          <div>
            <h2 id="pet-module-title" className="text-sm font-bold text-text-secondary">桌面伴随</h2>
            <p className="text-2xs text-text-muted">可选桌宠会同步鼠标与键盘输入，并显示轻量状态提示</p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Button
              variant="ghost"
              size="sm"
              loading={windowPlacementBusy === "bongo-cat"}
              disabled={!config.enable_bongo_cat || windowPlacementBusy !== null}
              onClick={onLocate}
              title="将窗口移到主界面所在屏幕"
            >
              <LocateFixed size={12} />
              定位
            </Button>
            <Toggle
              checked={!!config.enable_bongo_cat}
              onChange={async enabled => {
                updateConfig(current => { current.enable_bongo_cat = enabled; });
                const current = useGlobalConfig.getState().config;
                if (current) await persistConfig({ ...current, enable_bongo_cat: enabled }, true);
              }}
            />
          </div>
        </div>

        {config.enable_bongo_cat && (
          <div className="space-y-3 border-t border-border-default/50 pt-3">
            <div className="flex items-center justify-between gap-3">
              <div>
                <span className="text-sm font-semibold text-text-secondary">气泡对话框 (Chatterbox)</span>
                <p className="text-2xs text-text-muted">猫咪是否会偶尔冒出搞笑的台词以及系统状态提示</p>
              </div>
              <Toggle
                checked={!!config.bongo_cat_chatterbox}
                onChange={enabled => updateConfig(current => { current.bongo_cat_chatterbox = enabled; })}
              />
            </div>

            <div className="space-y-1">
              <div className="flex justify-between items-center text-xs">
                <label htmlFor="pet-scale" className="font-semibold text-text-secondary">猫咪显示缩放</label>
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
                    const current = useGlobalConfig.getState().config;
                    if (current) await persistConfig({ ...current, bongo_cat_scale: scale }, true);
                  } catch (error) {
                    console.error("保存猫咪缩放失败", error);
                  }
                }}
                className="w-full"
              />
            </div>

            <div className="flex items-center justify-between gap-3 pt-2">
              <div>
                <label htmlFor="pet-skin" className="text-sm font-semibold text-text-secondary">猫咪皮肤外观</label>
                <p className="text-2xs text-text-muted">当前选择的猫咪贴图类型</p>
              </div>
              <select
                id="pet-skin"
                value={config.bongo_cat_skin || "original"}
                onChange={event => updateConfig(current => { current.bongo_cat_skin = event.target.value; })}
                className="h-8 px-2.5 rounded-lg bg-surface-hover border border-border-default text-text-primary text-xs"
              >
                {(config.bongo_cat_unlocked_skins || ["original"]).map(skin => (
                  <option key={skin} value={skin}>{skin === "original" ? "经典原版" : skin}</option>
                ))}
              </select>
            </div>
          </div>
        )}
      </section>
    </div>
  );
}
