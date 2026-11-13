import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { invokeCommand } from "../../platform/tauri";
import { useModCapsulePool } from "./useModCapsulePool";
import { notifyModCatalogChanged } from "./catalogChanges";
import type { ModCapsulePool } from "../../store/types";

vi.mock("../../platform/tauri", () => ({ invokeCommand: vi.fn(), listenEvent: vi.fn() }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

it("refreshes the dashboard after another mounted manager mutates the catalog", async () => {
  let generation = 1;
  vi.mocked(invokeCommand).mockImplementation(async command => {
    if (command === "add_mod_capsule") generation = 2;
    return { generation, capsules: [], accounts: [] } as never;
  });
  const dashboard = renderHook(() => useModCapsulePool({ active: true }));
  const manager = renderHook(() => useModCapsulePool({ active: true }));
  await waitFor(() => expect(dashboard.result.current.pool?.generation).toBe(1));
  await act(async () => { await manager.result.current.add("CN", "-mod new -txt"); });
  await waitFor(() => expect(dashboard.result.current.pool?.generation).toBe(2));
  expect(manager.result.current.pool?.generation).toBe(2);
});

it("discards an in-flight stale read and refreshes once it completes", async () => {
  let complete!: (pool: ModCapsulePool) => void;
  vi.mocked(invokeCommand).mockImplementationOnce(() => new Promise(resolve => { complete = resolve as typeof complete; }));
  vi.mocked(invokeCommand).mockResolvedValue({ generation: 2, capsules: [], accounts: [] } as never);
  const dashboard = renderHook(() => useModCapsulePool({ active: true }));
  act(() => notifyModCatalogChanged());
  await act(async () => { complete({ generation: 1, capsules: [], accounts: [] } as unknown as ModCapsulePool); });
  await waitFor(() => expect(dashboard.result.current.pool?.generation).toBe(2));
  expect(invokeCommand).toHaveBeenCalledTimes(2);
});
