import {
  accountIdToDeleteOnCancel,
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
