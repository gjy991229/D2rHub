import {cleanup,render,screen,waitFor} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {afterEach,beforeEach,describe,expect,it,vi} from "vitest";
import UpdateConfirmModal from "./UpdateConfirmModal";
const invoke=vi.hoisted(()=>vi.fn());
const subscribe=vi.hoisted(()=>vi.fn());
vi.mock("../../platform/tauri",()=>({invokeCommand:invoke}));
vi.mock("../../features/tasks/taskSync",()=>({subscribeBeforeReadingTasks:subscribe}));
beforeEach(()=>{invoke.mockReset();subscribe.mockReset().mockResolvedValue(()=>{});});afterEach(cleanup);
describe("verified software updates",()=>{
  it("shows the shared themed progress bar with source status and percentage",async()=>{
    subscribe.mockImplementation(async(_gateway,listener)=>{
      listener(new Map([[1,{task_id:1,kind:"software-update-download",subject:"0.9.106",state:"running",progress:42,message:"正在从 Gitee 下载",cancel_requested:false}]]));
      return ()=>{};
    });
    render(<UpdateConfirmModal open onClose={()=>{}} version="0.9.106"/>);
    const bar=await screen.findByRole("progressbar",{name:"安装包下载进度"});
    expect(bar.className).toBe("hub-progress");
    expect(bar.getAttribute("aria-valuenow")).toBe("42");
    expect(screen.getByText("42%")).toBeTruthy();
    expect(screen.getByText("正在从 Gitee 下载")).toBeTruthy();
  });
  it("downloads first and starts the installer only after a separate user click",async()=>{
    invoke.mockResolvedValue({downloaded:true});const user=userEvent.setup();
    render(<UpdateConfirmModal open onClose={()=>{}} version="0.9.105" downloadUrl="https://untrusted.invalid/ignored.exe"/>);
    await user.click(screen.getByRole("button",{name:"下载安装包"}));
    await screen.findByRole("button",{name:"安装更新"});
    expect(invoke).toHaveBeenCalledWith("download_software_update",{version:"0.9.105"});
    expect(invoke).not.toHaveBeenCalledWith("launch_downloaded_update",expect.anything());
    await user.click(screen.getByRole("button",{name:"安装更新"}));
    await waitFor(()=>expect(invoke).toHaveBeenCalledWith("launch_downloaded_update",{version:"0.9.105"}));
  });
  it("never enables install when the download or integrity check fails",async()=>{
    invoke.mockRejectedValue(new Error("SHA-256 校验失败"));const user=userEvent.setup();
    render(<UpdateConfirmModal open onClose={()=>{}} version="0.9.105"/>);
    await user.click(screen.getByRole("button",{name:"下载安装包"}));
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.queryByRole("button",{name:"安装更新"})).toBeNull();
  });
});
