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
