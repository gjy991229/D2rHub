import "./roomQuickSettings.css";
import { useRef, useState } from "react";
import { useGlobalConfig } from "../../store/globalConfig";
import { useAccounts } from "../../store/accounts";
import { AnchoredPanel } from "../ui/AnchoredPanel";

export function RecognitionQuickEdit() {
  const { config, patch, saving } = useGlobalConfig();
  const accounts = useAccounts(state => state.accounts);
  const anchor = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [error, setError] = useState("");
  const english = config?.app_language === "en-US";
  if (!config) return null;
  return <div className="room-automation-quick">
    <button ref={anchor} type="button" className="control-btn" aria-haspopup="dialog" aria-expanded={open} data-active={open || undefined} onClick={() => setOpen(!open)}>{english ? "Recognition" : "识别配置"}</button>
    <AnchoredPanel open={open} anchor={anchor} title={english ? "Recognition account" : "识别账号"} onClose={() => setOpen(false)} width={340} closeLabel={english ? "Close" : "关闭"}>
      <div className="recognition-quick-content p-4 space-y-3">
        <p className="feature-help">{english ? "Choose one account. Changes apply immediately." : "选择一个识别主号，切换立即保存并应用。"}</p>
        {config.rune_audio_external_target && <p role="status">{english ? "Currently using an external game window." : "当前使用外部游戏窗口；选择账号后切换为该账号。"}</p>}
        {!accounts.some(account => account.initialized) && <p role="status">{english ? "Initialize an account to select it here." : "完成账号初始化后，即可在此选择。"}</p>}
        <div className="scheme-account-picker">{accounts.filter(account => account.initialized).map(account => <label key={account.id}>
          <input type="radio" name="recognition-account" disabled={saving} checked={!config.rune_audio_external_target && config.rune_audio_target_account === account.id} onChange={() => { setError(""); void patch({ rune_audio_target_account: account.id, rune_audio_external_target: null }).catch(error => setError(String(error))); }}/>{account.display_name || account.id}
        </label>)}</div>
        {saving && <p role="status">{english ? "Applying…" : "应用中…"}</p>}
        {error && <p role="alert" className="text-error">{error}</p>}
      </div>
    </AnchoredPanel>
  </div>;
}
