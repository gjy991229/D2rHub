import {act,cleanup,renderHook} from "@testing-library/react";
import {afterEach,beforeEach,describe,expect,it,vi} from "vitest";
import {useAutoUpdate} from "./useAppEffects";
import type {GlobalConfig} from "../store/types";
const invoke=vi.hoisted(()=>vi.fn());
vi.mock("../platform/tauri",()=>({invokeCommand:invoke,listenEvent:vi.fn()}));
vi.mock("../components/ui/Toast",()=>({showToast:vi.fn()}));
const config={first_run_complete:true,enable_auto_update:true} as GlobalConfig;
beforeEach(()=>{vi.useFakeTimers();localStorage.clear();invoke.mockReset();});
afterEach(()=>{cleanup();vi.useRealTimers();});
describe("daily update checks",()=>{
  it("checks software and resources independently under the same setting",async()=>{
    invoke.mockImplementation(async cmd=>cmd==="check_software_update"?{version:"0.9.103",available:false}:[]);
    const notify=vi.fn();renderHook(()=>useAutoUpdate(false,config,notify));
    await act(()=>vi.advanceTimersByTimeAsync(3000));
    expect(invoke).toHaveBeenCalledWith("check_software_update");expect(invoke).toHaveBeenCalledWith("check_mod_resource_updates");expect(notify).not.toHaveBeenCalled();
  });
  it("does not mark a failed software check complete or suppress a successful resource check",async()=>{
    invoke.mockImplementation(async cmd=>{if(cmd==="check_software_update")throw new Error("offline");return [];});
    renderHook(()=>useAutoUpdate(false,config,vi.fn()));await act(()=>vi.advanceTimersByTimeAsync(3000));
    expect(localStorage.getItem("d2rhub-v2-software-check-date")).toBeNull();expect(localStorage.getItem("d2rhub-v2-resources-check-date")).not.toBeNull();
  });
  it("disables both automatic checks when the existing setting is off",async()=>{
    renderHook(()=>useAutoUpdate(false,{...config,enable_auto_update:false},vi.fn()));await act(()=>vi.advanceTimersByTimeAsync(3000));expect(invoke).not.toHaveBeenCalled();
  });
});
