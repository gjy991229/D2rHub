import { useCallback, useEffect, useState, useRef } from "react";
import { ArrowLeft, ArrowRight, Check, Circle, ExternalLink, Globe2, KeyRound, Link2, Loader2, MonitorPlay, ShieldCheck, UserRound } from "lucide-react";
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
  resolveTokenAuthStepAction,
  shouldCancelInitializationOnClose,
  shouldStartBnetInitialization,
} from "../../utils/accountInitLifecycle";
import { extractBattleNetToken } from "../../utils/battleNetToken";
import { accountInitCopy } from "./accountInitCopy";
import "./accountInit.css";

interface Props {
  open: boolean;
  onClose: () => void;
  onDone: (accountId: string) => void;
  updateAccount?: AccountMeta | null;
  reinitializeAccount?: AccountMeta | null;
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
type TokenWizardStep = "token_nick" | "token_auth" | "token_guide" | "token_paste";
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

export function AccountInitDialog({ open, onClose, onDone, updateAccount, reinitializeAccount }: Props) {
  const { config } = useGlobalConfig();
  const en = config?.app_language === "en-US";
  const copy = accountInitCopy[en ? "en-US" : "zh-CN"];
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
  const [loginOpened, setLoginOpened] = useState(false);

  // Token wizard state
  const [tokenWizard, setTokenWizard] = useState<TokenWizardStep>("token_nick");
  const [tokenGuideLoading, setTokenGuideLoading] = useState(false);
  const [tokenAccountCreating, setTokenAccountCreating] = useState(false);
  const [tokenSubmitting, setTokenSubmitting] = useState(false);

  const cancelledRef = useRef(false);
  const accountIdRef = useRef("");
  const createdAccountIdRef = useRef("");
  const activeInitializationRef = useRef<Promise<void> | null>(null);
  const cancellingRef = useRef(false);
  const configuredBrowserOpenedRef = useRef(false);
  // 待完成账号的创建与提交都是异步的：ref 守卫保证同一帧内的重复点击不会创建第二个账号。
  const tokenAccountCreatingRef = useRef(false);
  const tokenSubmittingRef = useRef(false);
  // 向导会话号：取消或重新打开都会作废旧会话，迟到的创建结果必须由创建方回收。
  const wizardSessionRef = useRef(0);
  const finishedSessionRef = useRef<number | null>(null);
  const wizardOpenRef = useRef(false);
  const wizardTargetRef = useRef("");
  const nicknameRef = useRef<HTMLInputElement>(null);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const tokenInputRef = useRef<HTMLInputElement>(null);

  const {
    createAccount,
    deleteAccount,
    loadAccounts,
    initializeBnetAccount,
    reinitializeAccount: reinitializeExistingAccount,
  } = useAccounts();
  const cnBattleNetModeAvailable = hasConfiguredPathsForRegion(config, "CN", "bnet");

  const markDone = (step: InitStep) => setCompletedSteps(prev => new Set([...prev, step]));
  const finishInitialization = useCallback((session: number, id: string) => {
    if (!wizardOpenRef.current || session !== wizardSessionRef.current || finishedSessionRef.current === session) return;
    finishedSessionRef.current = session;
    onDone(id);
    onClose();
  }, [onDone, onClose]);

  useEffect(() => {
    const target = updateAccount?.id || reinitializeAccount?.id || "new";
    if (open && (!wizardOpenRef.current || wizardTargetRef.current !== target)) {
      wizardSessionRef.current += 1;
      finishedSessionRef.current = null;
      wizardTargetRef.current = target;
      tokenAccountCreatingRef.current = false;
      tokenSubmittingRef.current = false;
      setError(null);
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
      } else if (reinitializeAccount) {
        // 重新初始化以 Battle.net 客户端事务为主；若调用方误把 Token 账号交进来，
        // 则退化为“更新 Token”的登录页步骤，而不是渲染出空白向导。
        const reinitializeIsToken = reinitializeAccount.auth_mode === "token";
        setAccountId(reinitializeAccount.id);
        accountIdRef.current = reinitializeAccount.id;
        setNickname(reinitializeAccount.display_name || reinitializeAccount.id);
        setAuthMode(reinitializeIsToken ? "token" : "bnet");
        setToken("");
        setRegion(reinitializeAccount.region === "Global" ? "KR" : (reinitializeAccount.region as AccountRegion) || "CN");
        setLanguage(reinitializeAccount.language || "zhCN");
        setVoicelanguage(reinitializeAccount.voicelanguage || "zhCN");
        setNicknameLocked(!reinitializeIsToken);
        setTokenWizard(reinitializeIsToken ? "token_guide" : "token_nick");
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
      setLoginOpened(false);
      setTokenGuideLoading(false);
      setTokenAccountCreating(false);
      setTokenSubmitting(false);
      cancelledRef.current = false;
      createdAccountIdRef.current = "";
      activeInitializationRef.current = null;
      cancellingRef.current = false;
      configuredBrowserOpenedRef.current = false;
    }
    // Refreshes of the config or account list must not erase an in-progress form.
    if (!open && wizardOpenRef.current) wizardSessionRef.current += 1;
    wizardOpenRef.current = open;
  }, [open, updateAccount, reinitializeAccount, config?.cn_battle_net_path, config?.cn_game_path, config?.cn_saved_games_path, config?.global_game_path, config?.global_saved_games_path]);

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
  }, [open, config, nicknameLocked, currentStep, authMode, updateAccount, reinitializeAccount]);

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

  // Keep browser guidance inside the wizard; closed sessions must never open a page.
  useEffect(() => {
    if (!open || tokenWizard !== "token_guide") return;
    void tokenOpenGuide();
  }, [open, tokenWizard]);

  useEffect(() => {
    if (!open) return;
    if (tokenWizard === "token_nick") nicknameRef.current?.focus();
    else headingRef.current?.focus();
  }, [open, tokenWizard, nicknameLocked]);

  useEffect(() => {
    if (!open || currentStep !== "done") return;
    const session = wizardSessionRef.current;
    const timer = setTimeout(() => {
      finishInitialization(session, accountId);
    }, 1500);
    return () => clearTimeout(timer);
  }, [open, currentStep, accountId, finishInitialization]);

  const handleCancel = async () => {
    if (cancellingRef.current) return;
    cancellingRef.current = true;
    cancelledRef.current = true;
    // 先作废会话，再等待清理：期间返回的创建结果必须自行回收。
    wizardSessionRef.current += 1;
    setError(copy.cancelled);

    try {
      const activeInitialization = activeInitializationRef.current;
      const id = accountIdToDeleteOnCancel({
        isUpdating: Boolean(updateAccount || reinitializeAccount),
        createdAccountId: createdAccountIdRef.current,
      });
      const browserWasOpened = configuredBrowserOpenedRef.current;
      const cleanupAfterInitialization = async () => {
        if (browserWasOpened && config?.auto_close_browser && config.browser_type) {
          await invokeCommand("kill_browser_processes", { browserType: config.browser_type }).catch(() => {});
        }
        if (id) {
          await deleteAccount(id);
        }
      };

      const settled = authMode !== "bnet"
        || await requestInitializationCancellation(activeInitialization);
      if (!settled && activeInitialization) {
        showToast("warning", copy.cleanupPending);
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
      // 回到向导首步：既避免取消后仍触发 Token 登录页自动打开，也让下次打开重新初始化。
      setTokenWizard("token_nick");
      setLoginOpened(false);
      setTokenAccountCreating(false);
      setTokenSubmitting(false);
    }
  };

  const handleClose = () => {
    if (tokenSubmittingRef.current || currentStep === "done") return;
    if (shouldCancelInitializationOnClose({
      authMode,
      tokenWizard,
      currentStep,
      isReinitialize: Boolean(reinitializeAccount),
    })) {
      void handleCancel().finally(onClose);
      return;
    }
    onClose();
  };

  const runInit = async (): Promise<void> => {
    const session = wizardSessionRef.current;
    const isCurrentSession = () => session === wizardSessionRef.current && wizardOpenRef.current && !cancelledRef.current;
    if (!isCurrentSession()) return;
    setError(null);

    setCurrentStep("creating");
    let id = reinitializeAccount?.id || "";
    if (!id) {
      try {
        id = await createAccount(
          nickname.trim(),
          authMode,
          region,
          undefined,
          language || undefined,
          voicelanguage || undefined,
        );
        if (!isCurrentSession()) {
          // Cancellation may precede the create response and therefore cannot
          // capture this ID. The creating session must reclaim it itself.
          await deleteAccount(id);
          return;
        }
      } catch (accountCreationError) {
        if (isCurrentSession()) {
          setError(`${copy.createError}: ${String(accountCreationError)}`);
        }
        return;
      }

      accountIdRef.current = id;
      createdAccountIdRef.current = id;
      setAccountId(id);
    }
    markDone("creating");

    if (!isCurrentSession()) return;
    setCurrentStep("browser_setup");

    try {
      // 清理、启动、登录检测、快照采集和最终清理由一个后端事务完成。
      if (reinitializeAccount) {
        await reinitializeExistingAccount(id);
      } else {
        await initializeBnetAccount(id);
      }
      if (!isCurrentSession()) return;

      // 事件监听负责实时推进；这里补齐完成态，兼容事件监听初始化失败。
      markDone("browser_setup");
      markDone("browser_launch");
      markDone("launching_bnet");
      markDone("waiting_login");
      markDone("collecting");
      setCurrentStep("done");
      showToast("success", copy.created);
    } catch (initializationError) {
      if (!isCurrentSession()) return;
      setError(`${copy.initError}: ${String(initializationError)}`);
    }
  };
  // ── Token wizard handlers ──

  const tokenStepNickNext = () => {
    if (!nickname.trim()) { setError(copy.requiredNickname); nicknameRef.current?.focus(); return; }
    if (!hasConfiguredPathsForRegion(config, region, "token")) {
      setError(copy.pathError);
      return;
    }
    if (region !== "CN") setAuthMode("token");
    setError(null);
    setTokenWizard("token_auth");
  };

  const tokenStepAuthNext = async () => {
    if (authMode === "bnet") {
      if (region !== "CN") {
        setError(copy.cnOnly);
        return;
      }
      if (!hasConfiguredPathsForRegion(config, region, "bnet")) {
        setError(copy.bnetPathError);
        return;
      }
      // Bnet mode: lock and proceed to old flow
      handleConfirmNickname();
      return;
    }

    const action = resolveTokenAuthStepAction({
      hasAccount: Boolean(accountIdRef.current),
      isCreating: tokenAccountCreatingRef.current,
    });
    if (action === "open-guide") {
      setTokenWizard("token_guide");
      return;
    }
    if (action === "ignore") return;

    // Token 账号必须先落盘，登录页才能复用该账号的独立浏览器 Profile。
    // 账号此刻仍是待完成状态（initialized=false），取消时由这里负责回收。
    tokenAccountCreatingRef.current = true;
    setTokenAccountCreating(true);
    setError(null);
    const session = wizardSessionRef.current;
    try {
      const id = await createAccount(
        nickname.trim(),
        "token",
        region,
        undefined,
        language || undefined,
        voicelanguage || undefined,
        true,
      );
      if (session !== wizardSessionRef.current) {
        // 用户在创建期间取消（或已重新打开向导）：不推进流程，立即回收刚创建的待完成账号。
        await deleteAccount(id);
        return;
      }
      accountIdRef.current = id;
      createdAccountIdRef.current = id;
      setAccountId(id);
      setTokenWizard("token_guide");
    } catch (accountCreationError) {
      if (session === wizardSessionRef.current) {
        setError(`${copy.createError}: ${String(accountCreationError)}`);
      }
    } finally {
      if (session === wizardSessionRef.current) {
        tokenAccountCreatingRef.current = false;
        setTokenAccountCreating(false);
      }
    }
  };

  const handleConfirmNickname = () => {
    const trimmed = nickname.trim();
    if (!trimmed) { setError(copy.requiredNickname); return; }
    setError(null);
    setNickname(trimmed);
    markDone("input_nickname");
    setNicknameLocked(true);
  };

  const tokenOpenGuide = async () => {
    if (tokenGuideLoading) return;
    const session = wizardSessionRef.current;
    setTokenGuideLoading(true);
    setError(null);
    try {
      if (config?.browser_path && config?.browser_type) {
        const accountId = accountIdRef.current;
        if (!accountId) throw new Error(copy.missingAccount);
        await invokeCommand("open_token_login_url", { accountId, url: getTokenUrl(region) });
        if (session !== wizardSessionRef.current || !wizardOpenRef.current) {
          // A browser opened after cancellation belongs to the old session;
          // reclaim it here because cancellation could not observe it yet.
          if (config.auto_close_browser) {
            await invokeCommand("kill_browser_processes", { browserType: config.browser_type }).catch(() => {});
          }
          return;
        }
        configuredBrowserOpenedRef.current = true;
        await sleep(1200);
        if (session !== wizardSessionRef.current || !wizardOpenRef.current) return;
        await invokeCommand("bring_self_to_foreground").catch(() => {});
      } else {
        const { open: openUrl } = await import("@tauri-apps/plugin-shell");
        await openUrl(getTokenUrl(region));
      }
      if (session === wizardSessionRef.current && wizardOpenRef.current) setLoginOpened(true);
    } catch (e) {
      if (session !== wizardSessionRef.current || cancelledRef.current) return;
      setError(`${copy.loginError}: ${String(e)}`);
    } finally {
      if (session === wizardSessionRef.current && wizardOpenRef.current) {
        setTokenGuideLoading(false);
        setTokenWizard("token_paste");
      }
    }
  };

  const tokenStepPasteNext = async () => {
    if (tokenSubmittingRef.current) return;
    const extractedToken = extractBattleNetToken(token);
    if (!extractedToken) {
      setError(copy.invalidToken);
      return;
    }
    tokenSubmittingRef.current = true;
    setError(null);
    setTokenSubmitting(true);
    const updatingExistingAccount = Boolean(accountIdRef.current);
    const session = wizardSessionRef.current;
    try {
      let id = accountIdRef.current;
      if (id) {
        // 昵称在向导中不可编辑，这里只提交认证与区服/语言字段。
        await invokeCommand("update_account_meta", {
          accountId: id,
          authMode: "token",
          token: extractedToken,
          region,
          language,
          voicelanguage,
        });
        await loadAccounts();
      } else {
        id = await createAccount(
          nickname.trim(),
          "token",
          region,
          extractedToken,
          language || undefined,
          voicelanguage || undefined,
        );
        if (session !== wizardSessionRef.current) {
          // 与待完成账号同一规则：作废会话里创建的账号由创建方回收。
          await deleteAccount(id);
          return;
        }
        accountIdRef.current = id;
        createdAccountIdRef.current = id;
        setAccountId(id);

      }
      if (session !== wizardSessionRef.current) return;
      onDone(id);
      onClose();
      if (updateAccount) {
        showToast("success", migratingToToken ? copy.migrated : copy.updated);
      } else {
        showToast("success", copy.created);
      }
    } catch (e) {
      if (session !== wizardSessionRef.current) return;
      setError(`${updatingExistingAccount ? copy.updateError : copy.createError}: ${String(e)}`);
    } finally {
      if (session === wizardSessionRef.current) {
        tokenSubmittingRef.current = false;
        setTokenSubmitting(false);
      }
    }
  };

  const existingAccount = updateAccount || reinitializeAccount;
  const bnetRunning = nicknameLocked && authMode === "bnet";
  const connecting = tokenWizard === "token_guide" || tokenWizard === "token_paste";
  const stage = bnetRunning || connecting ? 2 : tokenWizard === "token_auth" ? 1 : 0;
  const busy = tokenAccountCreating || tokenSubmitting;
  const regions = [
    { value: "CN", label: copy.cn, edition: copy.cnEdition },
    { value: "KR", label: copy.asia, edition: copy.globalEdition },
    { value: "NA", label: copy.americas, edition: copy.globalEdition },
    { value: "EU", label: copy.europe, edition: copy.globalEdition },
  ] as const;
  const regionLabel = regions.find(item => item.value === region)?.label || region;
  const locales = [{ value: "zhCN", label: "简体中文" }, { value: "zhTW", label: "繁體中文" }, { value: "enUS", label: "English" }];

  return (
    <Modal open={open} onClose={handleClose} dismissible={!tokenSubmitting && currentStep !== "done"}
      initialFocusRef={existingAccount ? headingRef : nicknameRef}
      title={migratingToToken ? copy.migrateTitle : updateAccount ? copy.updateTitle : reinitializeAccount ? copy.reinitTitle : copy.title}
      width="max-w-xl"
      footer={<div className="account-init-footer">
        {currentStep === "done" ? <span className="account-init-local"><Check size={14} />{copy.created}</span>
          : <Button variant="ghost" size="sm" disabled={tokenSubmitting} onClick={handleClose}>{copy.cancel}</Button>}
        <div className="account-init-footer-actions">
          {tokenWizard === "token_auth" && !bnetRunning && <Button variant="secondary" size="sm" disabled={busy} onClick={() => { setError(null); setTokenWizard("token_nick"); }}><ArrowLeft size={14} />{copy.back}</Button>}
          {currentStep === "done" ? <Button variant="primary" size="sm" onClick={() => finishInitialization(wizardSessionRef.current, accountId)}>{copy.done}<Check size={14} /></Button>
            : bnetRunning ? null
            : connecting ? <Button variant="primary" size="sm" loading={tokenSubmitting} disabled={!extractBattleNetToken(token)} onClick={() => void tokenStepPasteNext()}>{tokenSubmitting ? copy.saving : existingAccount ? copy.save : copy.finish}<Check size={14} /></Button>
            : tokenWizard === "token_auth" ? <Button variant="primary" size="sm" loading={tokenAccountCreating} onClick={() => void tokenStepAuthNext()}>{tokenAccountCreating ? copy.creating : authMode === "bnet" ? copy.startBnet : copy.connect}<ArrowRight size={14} /></Button>
            : <Button variant="primary" size="sm" onClick={tokenStepNickNext}>{copy.next}<ArrowRight size={14} /></Button>}
        </div>
      </div>}
    >
      <div className="account-init" data-i18n-skip="true">
        {!existingAccount && <ol className="account-init-steps" aria-label={copy.progress}>
          {copy.steps.map((label, index) => <li key={label} data-state={index < stage ? "complete" : index === stage ? "active" : "pending"} aria-current={index === stage ? "step" : undefined}>
            <span className="account-init-step-number">{index < stage ? <Check size={13} /> : index + 1}</span><span>{label}</span>
          </li>)}
        </ol>}
        {stage > 0 && <div className="account-init-summary"><span className="account-init-avatar"><UserRound size={17} /></span><strong>{nickname}</strong><span className="account-init-summary-region"><Globe2 size={13} />{regionLabel}</span></div>}
        {!bnetRunning && <header className="account-init-heading">
          <h3 ref={headingRef} tabIndex={-1}>{stage === 0 ? copy.profileTitle : stage === 1 ? copy.authTitle : copy.connectTitle}</h3>
          <p>{stage === 0 ? copy.profileDescription : stage === 1 ? copy.authDescription : copy.connectDescription}</p>
        </header>}
        {stage === 0 && <div className="account-init-profile">
          <div>
            <Input id="account-init-nickname" ref={nicknameRef} label={copy.nickname} value={nickname} placeholder={copy.nicknamePlaceholder}
              aria-describedby="account-init-nickname-hint" onChange={e => { setNickname(e.target.value); setError(null); }}
              onKeyDown={e => { if (e.key === "Enter" && !e.nativeEvent.isComposing) tokenStepNickNext(); }} />
            <p id="account-init-nickname-hint" className="account-init-hint">{copy.nicknameHint}</p>
          </div>
          <fieldset className="account-init-fieldset">
            <legend>{copy.region}</legend><div className="account-init-regions">
              {regions.map(item => {
                const available = hasConfiguredPathsForRegion(config, item.value, "token");
                return <label key={item.value} className="account-init-region" data-selected={region === item.value} data-disabled={!available}>
                  <input type="radio" name="account-init-region" value={item.value} checked={region === item.value} disabled={!available} onChange={() => {
                    setRegion(item.value); setError(null);
                    if (item.value !== "CN") setAuthMode("token");
                    const locale = item.value === "CN" ? "zhCN" : item.value === "KR" ? "zhTW" : "enUS";
                    setLanguage(locale); setVoicelanguage(locale);
                  }} />
                  <span className="account-init-region-top"><Globe2 size={17} />{region === item.value && <Check size={14} />}</span>
                  <strong>{item.label}</strong><small>{available ? item.edition : copy.unavailable}</small>
                </label>;
              })}
            </div>
            {!firstConfiguredRegion(config, "token") && <p className="account-init-hint text-error">{copy.noPaths}</p>}
          </fieldset>
          <div className="account-init-languages">
            {[{ label: copy.language, name: "text", value: language, change: setLanguage }, { label: copy.voice, name: "voice", value: voicelanguage, change: setVoicelanguage }].map(group => <fieldset key={group.name} className="account-init-fieldset">
              <legend>{group.label}</legend><div className="account-init-segments">
                {locales.map(locale => <label key={locale.value} data-selected={group.value === locale.value}>
                  <input type="radio" name={"account-init-" + group.name} value={locale.value} checked={group.value === locale.value} onChange={() => group.change(locale.value)} /><span>{locale.label}</span>
                </label>)}
              </div>
            </fieldset>)}
          </div>
          <p className="account-init-hint">{copy.localeHint}</p>
        </div>}
        {stage === 1 && <fieldset className="account-init-auth account-init-fieldset" disabled={busy}>
          <legend className="sr-only">{copy.steps[1]}</legend>
          <label className="account-init-auth-option" data-selected={authMode === "token"}>
            <input type="radio" name="account-init-auth" value="token" checked={authMode === "token"} onChange={() => { setAuthMode("token"); setError(null); }} />
            <span className="account-init-auth-icon"><KeyRound size={21} /></span>
            <span className="account-init-auth-copy"><span className="account-init-auth-title"><strong>{copy.tokenTitle}</strong><small>{copy.recommended}</small></span><span>{copy.tokenDescription}</span></span>
            <span className="account-init-radio">{authMode === "token" && <Check size={12} />}</span>
          </label>
          {region === "CN" && <label className="account-init-auth-option" data-selected={authMode === "bnet"} data-disabled={!cnBattleNetModeAvailable}>
            <input type="radio" name="account-init-auth" value="bnet" checked={authMode === "bnet"} disabled={!cnBattleNetModeAvailable} onChange={() => { setAuthMode("bnet"); setError(null); }} />
            <span className="account-init-auth-icon"><MonitorPlay size={21} /></span>
            <span className="account-init-auth-copy"><span className="account-init-auth-title"><strong>{copy.bnetTitle}</strong><small>{copy.compatibility}</small></span><span>{cnBattleNetModeAvailable ? copy.bnetDescription : copy.bnetUnavailable}</span></span>
            <span className="account-init-radio">{authMode === "bnet" && <Check size={12} />}</span>
          </label>}
          <p className="account-init-notice"><ShieldCheck size={16} /><span>{authMode === "bnet" ? copy.bnetHint : region !== "CN" ? copy.globalHint : copy.tokenHint}</span></p>
        </fieldset>}
        {!bnetRunning && connecting && <div className="account-init-connect">
          <section className="account-init-login">
            <span className="account-init-login-icon">{tokenGuideLoading ? <Loader2 size={19} className="animate-spin" /> : <ExternalLink size={19} />}</span>
            <div><strong role="status">{tokenGuideLoading ? copy.opening : loginOpened ? copy.opened : copy.openFailed}</strong><p>{copy.loginInstruction}</p></div>
            <Button variant="secondary" size="sm" loading={tokenGuideLoading} disabled={tokenSubmitting} onClick={() => void tokenOpenGuide()}>{loginOpened ? copy.reopenLogin : copy.openLogin}</Button>
          </section>
          <details className="account-init-guide"><summary>{copy.guide}</summary><img src="/token-copy-guide.png" alt={copy.guideAlt} /></details>
          <div className="account-init-token">
            <label htmlFor="account-init-token"><Link2 size={15} />{copy.pasteLabel}</label>
            <Input id="account-init-token" ref={tokenInputRef} value={token} type="password" disabled={tokenSubmitting} autoComplete="off" spellCheck={false}
              aria-describedby="account-init-token-hint" placeholder={copy.pastePlaceholder}
              onChange={e => { const pasted = e.target.value; setToken(extractBattleNetToken(pasted) ?? pasted); setError(null); }}
              onKeyDown={e => { if (e.key === "Enter" && !e.nativeEvent.isComposing && extractBattleNetToken(token)) void tokenStepPasteNext(); }} />
            <p id="account-init-token-hint" className="account-init-hint">{copy.pasteHint}</p>
            {extractBattleNetToken(token) && <p className="account-init-token-ready" role="status"><Check size={14} />{copy.tokenReady}</p>}
          </div>
        </div>}
        {bnetRunning && <div className="account-init-bnet">
          <h3 ref={headingRef} tabIndex={-1}>{currentStep === "done" ? copy.created : copy.connectTitle}</h3><p>{copy.bnetProgress}</p>
          <ol aria-label={copy.progress}>
            {bnetSteps.filter(step => step.id !== "input_nickname").map((step, index) => {
              const done = completedSteps.has(step.id) || currentStep === "done";
              const active = step.id === currentStep;
              const labels = en ? [reinitializeAccount ? "Restore snapshot" : "Create account", "Prepare browser", "Open browser", "Open Battle.net", "Wait for sign-in", "Save snapshot", "Done"] : [reinitializeAccount ? "恢复账号快照" : "创建账号配置", "准备独立浏览器", "打开浏览器", "打开战网客户端", "等待完成登录", "保存认证快照", "完成"];
              return <li key={step.id} data-active={active} data-done={done}><span>{done ? <Check size={14} /> : active && !error ? <Loader2 size={14} className="animate-spin" /> : <Circle size={12} />}</span><span>{labels[index]}</span></li>;
            })}
          </ol>
        </div>}
        {error && <div className="account-init-error" role="alert">{error}</div>}
      </div>
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
