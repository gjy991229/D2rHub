import {
  accountIdToDeleteOnCancel,
  resolveTokenAuthStepAction,
  shouldCancelInitializationOnClose,
  shouldCleanupOnDialogClose,
  shouldStartBnetInitialization,
} from "./accountInitLifecycle";

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(`FAIL: ${message}`);
  console.log(`PASS: ${message}`);
}

export function runTests() {
  assert(
    !shouldStartBnetInitialization({
      open: true,
      hasConfig: true,
      nicknameLocked: true,
      currentStep: "input_nickname",
      authMode: "token",
      isUpdating: true,
    }),
    "updating an existing Token account never starts the Battle.net initialization flow",
  );

  assert(
    shouldStartBnetInitialization({
      open: true,
      hasConfig: true,
      nicknameLocked: true,
      currentStep: "input_nickname",
      authMode: "bnet",
      isUpdating: false,
    }),
    "a newly confirmed Battle.net account starts initialization",
  );

  assert(
    accountIdToDeleteOnCancel({ isUpdating: true, createdAccountId: "existing-account" }) === null,
    "cancelling a Token update never deletes the existing account",
  );
  assert(
    accountIdToDeleteOnCancel({ isUpdating: false, createdAccountId: "new-account" }) === "new-account",
    "cancelling a new account initialization cleans up only the newly created account",
  );

  assert(
    shouldCleanupOnDialogClose({ authMode: "token", tokenWizard: "token_guide", currentStep: "input_nickname" }),
    "closing an in-progress Token guide performs cleanup even though the Battle.net step is unchanged",
  );
  assert(
    !shouldCleanupOnDialogClose({ authMode: "token", tokenWizard: "token_nick", currentStep: "input_nickname" }),
    "closing before Token account creation does not run cleanup",
  );

  assert(
    shouldStartBnetInitialization({
      open: true,
      hasConfig: true,
      nicknameLocked: true,
      currentStep: "input_nickname",
      authMode: "bnet",
      isUpdating: false,
    }),
    "reinitializing a Battle.net account reuses the same initialization transaction as a first setup",
  );
  assert(
    accountIdToDeleteOnCancel({ isUpdating: true, createdAccountId: "" }) === null,
    "cancelling a reinitialization never deletes the account being rebuilt",
  );

  assert(
    !shouldCancelInitializationOnClose({
      authMode: "bnet",
      tokenWizard: "token_nick",
      currentStep: "input_nickname",
      isReinitialize: false,
    }),
    "closing a fresh wizard before it starts needs no cancellation",
  );
  assert(
    shouldCancelInitializationOnClose({
      authMode: "bnet",
      tokenWizard: "token_nick",
      currentStep: "input_nickname",
      isReinitialize: true,
    }),
    "closing a reinitialization before its first progress step still cancels the host transaction",
  );
  assert(
    !shouldCancelInitializationOnClose({
      authMode: "bnet",
      tokenWizard: "token_nick",
      currentStep: "done",
      isReinitialize: true,
    }),
    "closing a finished reinitialization keeps the committed snapshot",
  );
  assert(
    shouldCancelInitializationOnClose({
      authMode: "token",
      tokenWizard: "token_guide",
      currentStep: "input_nickname",
      isReinitialize: false,
    }),
    "closing an in-progress Token guide is still handled by the Token rules",
  );

  assert(
    resolveTokenAuthStepAction({ hasAccount: true, isCreating: false }) === "open-guide",
    "an already reserved Token account goes straight to the login guide",
  );
  assert(
    resolveTokenAuthStepAction({ hasAccount: false, isCreating: true }) === "ignore",
    "a second click while the Token account is being created is ignored",
  );
  assert(
    resolveTokenAuthStepAction({ hasAccount: false, isCreating: false }) === "create",
    "the first click creates exactly one pending Token account",
  );
}

const g = globalThis as any;
if (typeof g.process !== "undefined" && typeof g.process.argv !== "undefined") {
  try {
    runTests();
  } catch (error) {
    console.error(error);
    g.process.exit(1);
  }
}
