export interface BnetInitializationState {
  open: boolean;
  hasConfig: boolean;
  nicknameLocked: boolean;
  currentStep: string;
  authMode: "bnet" | "token";
  isUpdating: boolean;
}

export function shouldStartBnetInitialization(state: BnetInitializationState): boolean {
  return state.open
    && state.hasConfig
    && state.nicknameLocked
    && state.currentStep === "input_nickname"
    && state.authMode === "bnet"
    && !state.isUpdating;
}

export function accountIdToDeleteOnCancel({
  isUpdating,
  createdAccountId,
}: {
  isUpdating: boolean;
  createdAccountId: string;
}): string | null {
  if (isUpdating) return null;
  return createdAccountId || null;
}

export function shouldCleanupOnDialogClose({
  authMode,
  tokenWizard,
  currentStep,
}: {
  authMode: "bnet" | "token";
  tokenWizard: string;
  currentStep: string;
}): boolean {
  if (authMode === "token") return tokenWizard !== "token_nick";
  return currentStep !== "done" && currentStep !== "input_nickname";
}

/**
 * 关闭初始化弹窗是否需要显式下发取消。
 *
 * 重新初始化会占用宿主事务（最长等待 Battle.net 登录 120 秒），
 * 所以只要还没走到完成态，关闭窗口就必须取消，避免后台事务悬空。
 */
export function shouldCancelInitializationOnClose({
  authMode,
  tokenWizard,
  currentStep,
  isReinitialize,
}: {
  authMode: "bnet" | "token";
  tokenWizard: string;
  currentStep: string;
  isReinitialize: boolean;
}): boolean {
  return shouldCleanupOnDialogClose({ authMode, tokenWizard, currentStep })
    || (isReinitialize && currentStep !== "done");
}

export type TokenAuthStepAction = "open-guide" | "ignore" | "create";

/**
 * Token 认证“下一步”的去向。
 *
 * 待完成账号在进入登录页之前就已创建，因此已有账号时只推进向导；
 * 创建过程尚未返回时忽略重复点击，保证一个向导只会创建一个账号。
 */
export function resolveTokenAuthStepAction({
  hasAccount,
  isCreating,
}: {
  hasAccount: boolean;
  isCreating: boolean;
}): TokenAuthStepAction {
  if (hasAccount) return "open-guide";
  if (isCreating) return "ignore";
  return "create";
}
