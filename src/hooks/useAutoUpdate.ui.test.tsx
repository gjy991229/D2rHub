import {act,cleanup,renderHook} from "@testing-library/react";
import {afterEach,beforeEach,describe,expect,it,vi} from "vitest";
import {showToast} from "../components/ui/Toast";
import {useAutoUpdate} from "./useAppEffects";
import type {GlobalConfig} from "../store/types";
const invoke=vi.hoisted(()=>vi.fn());
vi.mock("../platform/tauri",()=>({invokeCommand:invoke,listenEvent:vi.fn()}));
vi.mock("../components/ui/Toast",()=>({showToast:vi.fn()}));
const config={first_run_complete:true,enable_auto_update:true} as GlobalConfig;
beforeEach(()=>{vi.useFakeTimers();localStorage.clear();invoke.mockReset();vi.mocked(showToast).mockClear();});
afterEach(()=>{cleanup();vi.useRealTimers();});
describe("daily update checks",()=>{
  it("checks software and resources independently under the same setting",async()=>{
    invoke.mockImplementation(async cmd=>cmd==="get_mod_capsule_pool"?{capsules:[]}:cmd==="check_software_update"?{version:"0.9.103",available:false}:[]);
    const notify=vi.fn();renderHook(()=>useAutoUpdate(false,config,notify));
    await act(()=>vi.advanceTimersByTimeAsync(3000));
    expect(invoke).toHaveBeenCalledWith("check_software_update");expect(invoke).toHaveBeenCalledWith("check_mod_resource_updates");expect(notify).not.toHaveBeenCalled();
  });
  it("does not mark a failed software check complete or suppress a successful resource check",async()=>{
    invoke.mockImplementation(async cmd=>{if(cmd==="check_software_update")throw new Error("offline");return cmd==="get_mod_capsule_pool"?{capsules:[]}:[];});
    renderHook(()=>useAutoUpdate(false,config,vi.fn()));await act(()=>vi.advanceTimersByTimeAsync(3000));
    expect(localStorage.getItem("d2rhub-v2-software-check-date")).toBeNull();expect(localStorage.getItem("d2rhub-v2-resources-check-date")).not.toBeNull();
  });
  it("checks local modules despite daily cache and reports old recipes",async()=>{
    const today=new Date().toLocaleDateString("en-CA");
    localStorage.setItem("d2rhub-v2-software-check-date",today);
    localStorage.setItem("d2rhub-v2-resources-check-date",today);
    invoke.mockResolvedValue({capsules:[{edition:"CN",name:"old-audio",update_required:true},{edition:"CN",name:"current",update_required:false}]});
    const {result}=renderHook(()=>useAutoUpdate(false,config,vi.fn()));
    await act(()=>vi.advanceTimersByTimeAsync(3000));
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("get_mod_capsule_pool");
    expect(result.current.modNotice).toBe(true);
    act(() => result.current.clearModNotice());
    expect(result.current.modNotice).toBe(false);
    expect(showToast).not.toHaveBeenCalledWith("info",expect.stringContaining("current"));
  });
  it("reports local module updates when online checks fail",async()=>{
    invoke.mockImplementation(async cmd=>{if(cmd==="get_mod_capsule_pool")return {capsules:[{edition:"Global",name:"old",update_required:true}]};throw new Error("offline");});
    const {result}=renderHook(()=>useAutoUpdate(false,config,vi.fn()));
    await act(()=>vi.advanceTimersByTimeAsync(3000));
    expect(result.current.modNotice).toBe(true);
    expect(showToast).not.toHaveBeenCalled();
  });
  it("does not notify for current modules",async()=>{
    invoke.mockImplementation(async cmd=>cmd==="get_mod_capsule_pool"?{capsules:[{update_required:false}]}:cmd==="check_software_update"?{available:false}:[]);
    renderHook(()=>useAutoUpdate(false,config,vi.fn()));
    await act(()=>vi.advanceTimersByTimeAsync(3000));
    expect(showToast).not.toHaveBeenCalled();
  });
  it("disables both automatic checks when the existing setting is off",async()=>{
    renderHook(()=>useAutoUpdate(false,{...config,enable_auto_update:false},vi.fn()));await act(()=>vi.advanceTimersByTimeAsync(3000));expect(invoke).not.toHaveBeenCalled();
  });
});
