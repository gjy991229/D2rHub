import { useEffect, useState, useRef } from "react";
import { createPortal } from "react-dom";
import { Check, Loader2, Circle } from "lucide-react";
import { Modal } from "../ui/Modal";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { useAccounts } from "../../store/accounts";
import { useGlobalConfig } from "../../store/globalConfig";
import { invokeCommand, listenEvent } from "../../platform/tauri";
import { showToast } from "../ui/Toast";
import type { AccountMeta, LaunchProgress } from "../../store/types";
import {
  firstConfiguredRegion,
  hasConfiguredPathsForRegion,
  requiresTokenMigration,
  type AccountRegion,
} from "../../utils/regionPaths";
import {
  accountIdToDeleteOnCancel,
  shouldCleanupOnDialogClose,
  shouldStartBnetInitialization,
} from "../../utils/accountInitLifecycle";
import { extractBattleNetToken } from "../../utils/battleNetToken";

interface Props {
  open: boolean;
  onClose: () => void;
  onDone: (accountId: string) => void;
  updateAccount?: AccountMeta | null;
}

type InitStep =
  | "input_nickname"
  | "creating"
  | "browser_setup"
  | "browser_launch"
  | "launching_bnet"
  | "waiting_login"
  | "collecting"
  | "done";

const bnetSteps = [
  { id: "input_nickname" as const, label: "输入昵称", desc: "为账号设置一个本地昵称" },
  { id: "creating" as const, label: "创建配置", desc: "创建本地账号存储目录" },
  { id: "browser_setup" as const, label: "浏览器配置", desc: "创建独立浏览器配置" },
  { id: "browser_launch" as const, label: "启动浏览器", desc: "打开浏览器" },
  { id: "launching_bnet" as const, label: "启动战网", desc: "打开 Battle.net 登录界面" },
  { id: "waiting_login" as const, label: "等待登录", desc: "请在战网中完成登录" },
  { id: "collecting" as const, label: "收集快照", desc: "保存认证与配置信息" },
  { id: "done" as const, label: "完成", desc: "" },
];

// ---------- Token wizard steps ----------
type TokenWizardStep = "token_nick" | "token_auth" | "token_guide" | "token_paste" | "token_settings";
const CANCEL_RETRY_INTERVAL_MS = 500;
const CANCEL_TIMEOUT_MS = 5_000;

const getTokenUrl = (region: string): string => {
  switch (region) {
    case "KR": return "https://kr.battle.net/login/en/?externalChallenge=login&app=OSI";
    case "NA": return "https://us.battle.net/login/en/?externalChallenge=login&app=OSI";
    case "EU": return "https://eu.battle.net/login/en/?externalChallenge=login&app=OSI";
    default: return "https://account.battlenet.com.cn/login/zh/?externalChallenge=login&app=OSI";
  }
};

const getTokenPrefix = (region: string): string => {
  switch (region) {
    case "NA": return "US";
    case "KR": return "KR";
    case "EU": return "EU";
    default: return "CN";
  }
};

export function AccountInitDialog({ open, onClose, onDone, updateAccount }: Props) {
  const { config } = useGlobalConfig();
  const migratingToToken = Boolean(updateAccount
    && requiresTokenMigration(updateAccount.auth_mode, updateAccount.region, config));
  const [currentStep, setCurrentStep] = useState<InitStep>("input_nickname");
  const [completedSteps, setCompletedSteps] = useState<Set<InitStep>>(new Set());
  const [accountId, setAccountId] = useState<string>("");
  const [error, setError] = useState<string | null>(null);
  const [nickname, setNickname] = useState("");
  const [authMode, setAuthMode] = useState<"bnet" | "token">("token");
  const [region, setRegion] = useState<AccountRegion>("CN");
  const [token, setToken] = useState("");
  const [language, setLanguage] = useState("zhCN");
  const [voicelanguage, setVoicelanguage] = useState("zhCN");
  const [nicknameLocked, setNicknameLocked] = useState(false);
  const [showGuide, setShowGuide] = useState(false);

  // Token wizard state
  const [tokenWizard, setTokenWizard] = useState<TokenWizardStep>("token_nick");
  const [tokenGuideLoading, setTokenGuideLoading] = useState(false);
  const [tokenSubmitting, setTokenSubmitting] = useState(false);

  const cancelledRef = useRef(false);
  const accountIdRef = useRef("");
  const createdAccountIdRef = useRef("");
  const activeInitializationRef = useRef<Promise<void> | null>(null);
  const cancellingRef = useRef(false);

  const { createAccount, initializeBnetAccount } = useAccounts();
  const cnBattleNetModeAvailable = hasConfiguredPathsForRegion(config, "CN", "bnet");

  const markDone = (step: InitStep) => setCompletedSteps(prev => new Set([...prev, step]));

  useEffect(() => {
    if (open) {
      setCurrentStep("input_nickname");
      setCompletedSteps(new Set());
      if (updateAccount) {
        setAccountId(updateAccount.id);
        accountIdRef.current = updateAccount.id;
        setNickname(updateAccount.display_name || updateAccount.id);
        setAuthMode("token");
        let initialRegion: "CN" | "KR" | "NA" | "EU" = "CN";
        if (updateAccount.region === "Global") {
          initialRegion = "KR";
        } else if (["CN", "KR", "NA", "EU"].includes(updateAccount.region as string)) {
          initialRegion = updateAccount.region as "CN" | "KR" | "NA" | "EU";
        } else if (!updateAccount.region?.trim()) {
          initialRegion = firstConfiguredRegion(config, "token") || "CN";
        }
        setRegion(initialRegion);
        setToken("");
        const initialLocale = initialRegion === "CN" ? "zhCN" : initialRegion === "KR" ? "zhTW" : "enUS";
        setLanguage(updateAccount.language || initialLocale);
        setVoicelanguage(updateAccount.voicelanguage || initialLocale);
        setNicknameLocked(true);
        setTokenWizard("token_guide");
      } else {
        setAccountId("");
        accountIdRef.current = "";
        setNickname("");
        setAuthMode("token");
        const initialRegion = firstConfiguredRegion(config, "token") || "CN";
        setRegion(initialRegion);
        setToken("");
        const initialLocale = initialRegion === "CN" ? "zhCN" : initialRegion === "KR" ? "zhTW" : "enUS";
        setLanguage(initialLocale);
        setVoicelanguage(initialLocale);
        setNicknameLocked(false);
        setTokenWizard("token_nick");
      }
      setShowGuide(false);
      setTokenGuideLoading(false);
      setTokenSubmitting(false);
      cancelledRef.current = false;
      createdAccountIdRef.current = "";
      activeInitializationRef.current = null;
      cancellingRef.current = false;
    }
  }, [open, updateAccount, config?.cn_battle_net_path, config?.cn_game_path, config?.cn_saved_games_path, config?.global_game_path, config?.global_saved_games_path]);

  useEffect(() => {
    if (!shouldStartBnetInitialization({
      open,
      hasConfig: Boolean(config),
      nicknameLocked,
      currentStep,
      authMode,
      isUpdating: Boolean(updateAccount),
    })) return;
    const initialization = runInit();
    activeInitializationRef.current = initialization;
    const clearActive = () => {
      if (activeInitializationRef.current === initialization) {
        activeInitializationRef.current = null;
      }
    };
    void initialization.then(clearActive, clearActive);
  }, [open, config, nicknameLocked, currentStep, authMode, updateAccount]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenEvent<LaunchProgress>("launch-progress", ({ payload }) => {
      if (payload.account_id !== accountIdRef.current) return;
      const succeeded = payload.status === "ok";
      switch (payload.step) {
        case "browser":
          if (!succeeded) setCurrentStep("browser_setup");
          if (succeeded) {
            markDone("browser_setup");
            markDone("browser_launch");
          }
          break;
        case "launch":
          if (!succeeded) setCurrentStep("launching_bnet");
          if (succeeded) markDone("launching_bnet");
          break;
        case "login":
          setCurrentStep("waiting_login");
          if (succeeded) markDone("waiting_login");
          break;
        case "snapshot":
          setCurrentStep("collecting");
          if (succeeded) markDone("collecting");
          break;
      }
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch((eventError) => {
      console.error("Failed to listen for account initialization progress:", eventError);
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  // Auto-trigger browser launch when entering token guide step
  useEffect(() => {
    if (tokenWizard !== "token_guide" || showGuide) return;
    tokenOpenGuide();
  }, [tokenWizard]);

  useEffect(() => {
    if (currentStep !== "done") return;
    const timer = setTimeout(() => {
      onDone(accountId);
      onClose();
    }, 1500);
    return () => clearTimeout(timer);
  }, [currentStep]);

  const handleCancel = async () => {
    if (cancellingRef.current) return;
    cancellingRef.current = true;
    cancelledRef.current = true;
    setError("已取消初始化流程");

    try {
      const activeInitialization = activeInitializationRef.current;
      const id = accountIdToDeleteOnCancel({
        isUpdating: Boolean(updateAccount),
        createdAccountId: createdAccountIdRef.current,
      });
      const cleanupAfterInitialization = async () => {
        if (config?.auto_close_browser && config.browser_type) {
          await invokeCommand("kill_browser_processes", { browserType: config.browser_type }).catch(() => {});
        }
        if (id) {
          await invokeCommand("delete_account", { accountId: id });
        }
      };

      const settled = authMode !== "bnet"
        || await requestInitializationCancellation(activeInitialization);
      if (!settled && activeInitialization) {
        showToast("warning", "取消请求已发送，后台仍在安全清理；完成后会自动删除未完成账号");
        void activeInitialization
          .catch(() => {})
          .then(cleanupAfterInitialization)
          .catch((cleanupError) => console.error("Deferred cancel cleanup failed:", cleanupError));
      } else {
        await activeInitialization?.catch(() => {});
        await cleanupAfterInitialization();
      }
    } catch (e) {
      console.error("Cancel cleanup failed:", e);
    } finally {
      activeInitializationRef.current = null;
      setCurrentStep("input_nickname");
      setNicknameLocked(false);
      setCompletedSteps(new Set());
      setAccountId("");
      accountIdRef.current = "";
      createdAccountIdRef.current = "";
      cancellingRef.current = false;
    }
  };

  const handleClose = () => {
    if (shouldCleanupOnDialogClose({ authMode, tokenWizard, currentStep })) {
      void handleCancel().finally(onClose);
      return;
    }
    onClose();
  };

  const runInit = async (): Promise<void> => {
    setError(null);
    if (cancelledRef.current) return;

    setCurrentStep("creating");
    let id: string;
    try {
      id = await createAccount(
        nickname.trim(),
        authMode,
        region,
        undefined,
        language || undefined,
        voicelanguage || undefined,
      );
    } catch (accountCreationError) {
      if (!cancelledRef.current) {
        setError(`创建账号失败: ${String(accountCreationError)}`);
      }
      return;
    }

    accountIdRef.current = id;
    createdAccountIdRef.current = id;
    setAccountId(id);
    markDone("creating");

    if (cancelledRef.current) return;
    setCurrentStep("browser_setup");

    try {
      // 清理、启动、登录检测、快照采集和最终清理由一个后端事务完成。
      await initializeBnetAccount(id);
      if (cancelledRef.current) return;

      // 事件监听负责实时推进；这里补齐完成态，兼容事件监听初始化失败。
      markDone("browser_setup");
      markDone("browser_launch");
      markDone("launching_bnet");
      markDone("waiting_login");
      markDone("collecting");
      setCurrentStep("done");
      showToast("success", "账号 " + nickname.trim() + " 初始化完成！");
    } catch (initializationError) {
      if (cancelledRef.current) return;
      setError("账号初始化失败: " + String(initializationError));
    }
  };
  // ── Token wizard handlers ──

  const tokenStepNickNext = () => {
    if (!nickname.trim()) { setError("请输入昵称"); return; }
    setError(null);
    setTokenWizard("token_settings");
  };

  const tokenStepSettingsNext = () => {
    if (!hasConfiguredPathsForRegion(config, region, "token")) {
      setError(`${region === "CN" ? "国服" : "国际服"}游戏安装目录尚未配置，请先前往设置补全`);
      return;
    }
    if (region !== "CN") setAuthMode("token");
    setError(null);
    setTokenWizard("token_auth");
  };

  const tokenStepAuthNext = () => {
    if (authMode === "bnet") {
      if (region !== "CN") {
        setError("国际服仅支持 Token 直启");
        return;
      }
      if (!hasConfiguredPathsForRegion(config, region, "bnet")) {
        setError("国服 Battle.net.exe 尚未配置，战网认证无法继续");
        return;
      }
      // Bnet mode: lock and proceed to old flow
      handleConfirmNickname();
    } else {
      setTokenWizard("token_guide");
    }
  };

  const handleConfirmNickname = () => {
    const trimmed = nickname.trim();
    if (!trimmed) { setError("请输入昵称"); return; }
    setError(null);
    setNickname(trimmed);
    markDone("input_nickname");
    setNicknameLocked(true);
  };

  const tokenOpenGuide = async () => {
    setTokenGuideLoading(true);
    setError(null);
    try {
      if (config?.browser_path && config?.browser_type) {
        await invokeCommand("open_token_login_url", { url: getTokenUrl(region) });
        await sleep(1200);
        await invokeCommand("bring_self_to_foreground").catch(() => {});
      } else {
        const { open: openUrl } = await import("@tauri-apps/plugin-shell");
        await openUrl(getTokenUrl(region));
      }
      setShowGuide(true);
    } catch (e) {
      if (cancelledRef.current) return;
      setError(`打开 Token 登录页面失败: ${String(e)}`);
    } finally {
      setTokenGuideLoading(false);
    }
  };

  const handleGuideClose = () => {
    setShowGuide(false);
    setTokenWizard("token_paste");
  };

  const handleOpenTokenWeb = async (e?: React.MouseEvent) => {
    if (e) e.stopPropagation();
    const tokenUrl = getTokenUrl(region);
    try {
      if (config?.browser_path && config?.browser_type) {
        await invokeCommand("open_token_login_url", { url: tokenUrl });
      } else {
        const { open: openUrl } = await import("@tauri-apps/plugin-shell");
        await openUrl(tokenUrl);
      }
    } catch (err) {
      console.error("无法打开网页:", err);
      showToast("error", "打开外部浏览器失败，请手动网页打开。");
    }
  };

  const tokenStepPasteNext = async () => {
    if (tokenSubmitting) return;
    const extractedToken = extractBattleNetToken(token);
    if (!extractedToken) {
      setError("未找到完整 Token，请粘贴包含 ST=...& 的完整链接");
      return;
    }
    setError(null);
    setTokenSubmitting(true);
    const updatingExistingAccount = Boolean(accountIdRef.current);
    try {
      let id = accountIdRef.current;
      if (id) {
        await invokeCommand("update_account_meta", {
          accountId: id,
          authMode: "token",
          token: extractedToken,
          region,
          language,
          voicelanguage,
        });
      } else {
        id = await createAccount(
          nickname.trim(),
          "token",
          region,
          extractedToken,
          language || undefined,
          voicelanguage || undefined,
        );
        accountIdRef.current = id;
        createdAccountIdRef.current = id;
        setAccountId(id);
      }
      onDone(id);
      onClose();
      if (updateAccount) {
        showToast("success", migratingToToken ? "已迁移为 Token 直启！" : "Token 已更新！");
      } else {
        showToast("success", "Token 账号 " + nickname.trim() + " 初始化完成！");
      }
    } catch (e) {
      const action = updatingExistingAccount ? "更新账号配置" : "创建账号";
      setError(`${action}失败: ${String(e)}`);
    } finally {
      setTokenSubmitting(false);
    }
  };

  // ── Render ──

  return (
    <Modal
      open={open}
      onClose={handleClose}
      title={migratingToToken ? "迁移为 Token 直启" : updateAccount ? "更新账号 Token" : "初始化新账号"}
      width="max-w-md"
      footer={
        currentStep === "done" ? (
          <Button variant="primary" size="sm" onClick={() => { onDone(accountId); onClose(); }}>完成</Button>
        ) : (currentStep !== "input_nickname" || tokenWizard !== "token_nick") ? (
          <Button variant="secondary" size="sm" onClick={handleCancel}>取消</Button>
        ) : null
      }
    >
      {/* ═══════════ Token Wizard Flow ═══════════ */}
      {(!nicknameLocked || Boolean(updateAccount)) && (
        <div className="mb-3 flex flex-col gap-3">
          {/* Step indicator */}
          <div className="flex items-center gap-2 text-xs text-text-muted">
            <span className={tokenWizard === "token_nick" ? "text-accent font-bold" : ""}>1.昵称</span>
            <span>→</span>
            <span className={tokenWizard === "token_settings" ? "text-accent font-bold" : ""}>2.设置</span>
            <span>→</span>
            <span className={tokenWizard === "token_auth" ? "text-accent font-bold" : ""}>3.模式</span>
            <span>→</span>
            <span className={tokenWizard === "token_guide" ? "text-accent font-bold" : ""}>4.获取Token</span>
            <span>→</span>
            <span className={tokenWizard === "token_paste" ? "text-accent font-bold" : ""}>5.粘贴Token</span>
          </div>

          {/* Step 1: Nickname */}
          {tokenWizard === "token_nick" && (
            <>
              <div>
                <p className="text-md text-text-secondary mb-1.5">设置昵称（用于本地标识）</p>
                <Input
                  value={nickname}
                  onChange={e => { setNickname(e.target.value); setError(null); }}
                  placeholder="例如：主号、小号1"
                  autoFocus
                />
              </div>
              <Button variant="primary" size="sm" onClick={tokenStepNickNext}>下一步</Button>
            </>
          )}

          {/* Step 2: Settings — sliders for region/language/voice */}
          {tokenWizard === "token_settings" && (
            <>
              <div className="flex flex-col gap-4">
                {/* Region buttons */}
                <div>
                  <p className="text-sm text-text-secondary mb-1.5">区服</p>
                  <div className="grid grid-cols-4 gap-1.5">
                    {(["CN", "KR", "NA", "EU"] as const).map(r => (
                      <button
                        key={r}
                        type="button"
                        disabled={!hasConfiguredPathsForRegion(config, r, "token")}
                        title={!hasConfiguredPathsForRegion(config, r, "token")
                          ? `${r === "CN" ? "国服" : "国际服"}游戏安装目录尚未配置`
                          : undefined}
                        onClick={() => {
                          setRegion(r);
                          if (r !== "CN") setAuthMode("token");
                          if (r === "CN") {
                            setLanguage("zhCN");
                            setVoicelanguage("zhCN");
                          } else if (r === "KR") {
                            setLanguage("zhTW");
                            setVoicelanguage("zhTW");
                          } else {
                            setLanguage("enUS");
                            setVoicelanguage("enUS");
                          }
                        }}
                        className={`py-2 px-1.5 rounded-xl text-xs font-medium transition-all duration-200 ${
                          region === r
                            ? "bg-accent text-white shadow-sm scale-[1.03]"
                            : "bg-surface-hover text-text-secondary hover:bg-surface-active"
                        } disabled:opacity-35 disabled:cursor-not-allowed disabled:hover:bg-surface-hover`}
                      >
                        {r === "CN" && "国服"}
                        {r === "KR" && "亚服"}
                        {r === "NA" && "美服"}
                        {r === "EU" && "欧服"}
                      </button>
                    ))}
                  </div>
                  {!firstConfiguredRegion(config, "token") && (
                    <p className="text-xs text-error mt-2">尚未配置任何版本的游戏安装目录，暂时无法创建账号。</p>
                  )}
                </div>

                {/* Language buttons */}
                <div>
                  <p className="text-sm text-text-secondary mb-1.5">界面语言</p>
                  <div className="flex gap-2">
                    {([
                      { v: "zhCN", label: "简体中文" },
                      { v: "zhTW", label: "繁体中文" },
                      { v: "enUS", label: "English" },
                    ]).map(l => (
                      <button
                        key={l.v}
                        onClick={() => setLanguage(l.v)}
                        className={`flex-1 py-2.5 px-2 rounded-xl text-sm font-medium transition-all duration-200 ${
                          language === l.v
                            ? "bg-accent text-white shadow-sm scale-[1.03]"
                            : "bg-surface-hover text-text-secondary hover:bg-surface-active"
                        }`}
                      >
                        {l.label}
                      </button>
                    ))}
                  </div>
                </div>

                {/* Voice language buttons */}
                <div>
                  <p className="text-sm text-text-secondary mb-1.5">配音语言</p>
                  <div className="flex gap-2">
                    {([
                      { v: "zhCN", label: "简体中文" },
                      { v: "zhTW", label: "繁体中文" },
                      { v: "enUS", label: "English" },
                    ]).map(vl => (
                      <button
                        key={vl.v}
                        onClick={() => setVoicelanguage(vl.v)}
                        className={`flex-1 py-2.5 px-2 rounded-xl text-sm font-medium transition-all duration-200 ${
                          voicelanguage === vl.v
                            ? "bg-accent text-white shadow-sm scale-[1.03]"
                            : "bg-surface-hover text-text-secondary hover:bg-surface-active"
                        }`}
                      >
                        {vl.label}
                      </button>
                    ))}
                  </div>
                </div>
              </div>
              <Button variant="primary" size="sm" onClick={tokenStepSettingsNext}>下一步</Button>
            </>
          )}

          {/* Step 3: Auth mode */}
          {tokenWizard === "token_auth" && (
            <>
              <p className="text-md text-text-secondary mb-1.5">选择认证模式</p>
              <div className="flex flex-col gap-2">
                <label
                  className="flex items-center gap-2 cursor-pointer p-3 rounded-xl border border-accent/30 bg-surface-hover"
                  title="优点：无需战网客户端，零秒启动，长期有效。缺点：需要战网账号（非网易账号）；若通过网易注册的战网号，可能需要用手机号设置战网密码才能登录网页获取 Token。"
                >
                  <input type="radio" checked={authMode === "token"} onChange={() => { setAuthMode("token"); setError(null); }} className="accent-accent" />
                  <span className="text-md">✨ 网页 Token 认证（推荐·免战网）</span>
                </label>
                {region === "CN" ? (
                  <label
                    className={`flex items-center gap-2 p-3 rounded-xl border border-border-default ${
                      cnBattleNetModeAvailable
                        ? "cursor-pointer"
                        : "cursor-not-allowed opacity-50"
                    }`}
                    title={cnBattleNetModeAvailable
                      ? "国服兼容模式。配置简单，仅需网易账号，但启动较慢且需要经过战网客户端。"
                      : "国服 Battle.net.exe 尚未完整配置"}
                  >
                    <input
                      type="radio"
                      checked={authMode === "bnet"}
                      disabled={!cnBattleNetModeAvailable}
                      onChange={() => { setAuthMode("bnet"); setError(null); }}
                      className="accent-accent disabled:cursor-not-allowed"
                    />
                    <span className="text-md">战网客户端认证（国服兼容）</span>
                  </label>
                ) : (
                  <p className="text-xs text-text-muted px-1 leading-relaxed">
                    国际服固定使用 Token 直启，不启动 Battle.net 客户端，避免多客户端冲突。
                  </p>
                )}
                {authMode === "bnet" && (
                  <p className="text-xs text-text-muted px-1">
                    战网认证沿用该账号的客户端快照语言；上一步的语言与配音选择仅用于 Token 直启。
                  </p>
                )}
              </div>
              <Button variant="primary" size="sm" onClick={tokenStepAuthNext}>下一步</Button>
            </>
          )}

          {/* Step 4: Guide (browser auto-launches) */}
          {tokenWizard === "token_guide" && !showGuide && (
            <div className="flex flex-col gap-3">
              <div className="flex items-center gap-2 text-accent">
                <Loader2 size={16} className="animate-spin" />
                <span className="text-sm">正在打开 Token 登录页面...</span>
              </div>
              {tokenGuideLoading && (
                <p className="text-xs text-text-muted">浏览器启动后，请在新打开的登录页中获取 Token，软件将自动弹出指引图</p>
              )}
              {!tokenGuideLoading && (
                <Button variant="primary" size="sm" onClick={tokenOpenGuide}>重新尝试打开浏览器</Button>
              )}
            </div>
          )}

          {/* Step 5: Paste the complete redirect URL and extract its ST value. */}
          {tokenWizard === "token_paste" && (
            <>
              <div>
                <p className="text-md text-text-secondary mb-1.5">粘贴完整链接</p>
                <Input value={token} onChange={e => {
                  const pasted = e.target.value;
                  setToken(extractBattleNetToken(pasted) ?? pasted);
                  setError(null);
                }} placeholder={`粘贴包含 ST=${getTokenPrefix(region)}-...& 的完整链接`} autoFocus autoComplete="off" spellCheck={false} />
                <p className="text-xs text-text-muted mt-2 leading-relaxed">
                  软件会自动提取 ST= 与下一个 &amp; 之间的 Token；复制内容中夹带中文说明也可以识别。
                </p>
              </div>
              <Button variant="primary" size="sm" onClick={tokenStepPasteNext} disabled={!extractBattleNetToken(token) || tokenSubmitting}>
                {tokenSubmitting ? "正在保存..." : "确认完成"}
              </Button>
            </>
          )}
        </div>
      )}

      {/* ═══════════ Old bnet flow (unchanged) ═══════════ */}
      {nicknameLocked && authMode === "bnet" && (
        <div className="relative pl-1">
          <div className="absolute left-[17px] top-3 bottom-3 w-px"
            style={{ background: "var(--border-default)" }} />
          <div className="space-y-0.5">
            {bnetSteps.filter(s => s.id !== "input_nickname").map(s => {
              const done = completedSteps.has(s.id);
              const active = s.id === currentStep;
              return (
                <div key={s.id} className="relative flex items-center gap-3.5 py-2">
                  <div className={"shrink-0 w-[34px] h-[34px] rounded-full flex items-center justify-center z-10 transition-all duration-300 " + (
                    done ? "bg-success/10 border-2 border-success/30"
                      : active ? "bg-accent/10 border-2 border-accent/30"
                        : "border border-border-default bg-surface-base"
                  )}>
                    {done ? <Check size={14} className="text-success" />
                      : active ? <Loader2 size={14} className="animate-spin text-accent" />
                        : <Circle size={14} className="text-text-muted/20" />
                    }
                  </div>
                  <div>
                    <p className={"text-md font-medium transition-colors " + (
                      done ? "text-success" : active ? "text-text-primary" : "text-text-muted"
                    )}>{s.label}</p>
                    {s.desc && active && <p className="text-xs text-text-muted mt-0.5">{s.desc}</p>}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* ═══════════ Error ═══════════ */}
      {error && (
        <div className="mt-3 p-3 rounded-xl text-md text-error"
          style={{ background: "rgba(224,96,96,0.08)", border: "1px solid rgba(224,96,96,0.15)" }}>
          {error}
        </div>
      )}

      {/* ═══════════ BIG Guide Overlay (Portal to body) ═══════════ */}
      {showGuide && createPortal(
        <div className="fixed inset-0 z-[200] flex items-center justify-center bg-[rgba(18,24,34,0.38)] backdrop-blur-[5px]" onClick={(e) => { e.stopPropagation(); handleGuideClose(); }}>
          <div className="relative bg-surface-elevated rounded-modal p-6 max-w-3xl w-[95vw] mx-4 shadow-elevated border border-border-default" onClick={e => e.stopPropagation()}>
            <p className="text-xl font-bold text-text-primary mb-4 text-center">🔍 请在浏览器中复制完整链接</p>
            <img src="/token-copy-guide.png" alt="完整链接复制指引" className="w-full rounded-xl border border-border-default" style={{ maxHeight: "80vh", objectFit: "contain" }} />
            <p className="text-sm text-text-muted mt-4 text-center">复制完成后关闭此弹窗，在下一步粘贴完整链接</p>
            <div className="flex gap-2 mt-4">
              <Button variant="secondary" size="sm" className="flex-1" onClick={handleOpenTokenWeb}>
                手动打开 Token 网页
              </Button>
              <Button variant="primary" size="sm" className="flex-1" onClick={(e) => { e.stopPropagation(); handleGuideClose(); }}>
                已复制完整链接
              </Button>
            </div>
          </div>
        </div>,
        document.body
      )}
    </Modal>
  );
}

function sleep(ms: number) { return new Promise<void>(resolve => setTimeout(resolve, ms)); }

async function requestInitializationCancellation(
  initialization: Promise<void> | null,
): Promise<boolean> {
  if (!initialization) return true;
  const settled = initialization.then(() => true, () => true);
  const attempts = Math.ceil(CANCEL_TIMEOUT_MS / CANCEL_RETRY_INTERVAL_MS);

  for (let attempt = 0; attempt < attempts; attempt += 1) {
    void invokeCommand("cancel_launch").catch((error) => {
      console.warn("Failed to send initialization cancellation:", error);
    });
    if (await Promise.race([settled, sleep(CANCEL_RETRY_INTERVAL_MS).then(() => false)])) {
      return true;
    }
  }
  return false;
}
