import { useRef } from "react";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it } from "vitest";
import { ModBatchPanel } from "./ModBatchPanel";
import { useBatchProcessing } from "../../mods/workflow/useBatchProcessing";
import { testCatalog, workflowFixture } from "../../mods/workflow/testFixtures";

afterEach(cleanup);

it("shows and locks Double Esc when hostile auto-restart requires it", async () => {
  function BatchFixture() {
    const batch = useBatchProcessing([], testCatalog(), useRef(false), false);
    return <ModBatchPanel workflow={workflowFixture({ batch })} />;
  }
  render(<BatchFixture />);
  const escape = screen.getByRole("checkbox", { name: /^双 Esc/ }) as HTMLInputElement;
  expect(escape.checked).toBe(false);
  expect(escape.disabled).toBe(false);
  await userEvent.click(screen.getByRole("checkbox", { name: /^敌对自动重开/ }));
  expect(escape.checked).toBe(true);
  expect(escape.disabled).toBe(true);
  expect(screen.getByText("敌对自动重开必选")).toBeTruthy();
});
