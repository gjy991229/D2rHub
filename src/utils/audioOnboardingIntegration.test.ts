export {};

declare const require: (name: string) => unknown;
declare const __dirname: string;

const fs = require("fs") as { readFileSync(path: string, encoding: string): string };
const path = require("path") as { join(...parts: string[]): string };
const source = fs.readFileSync(
  path.join(__dirname, "../features/settings/panels/AutomationPanel.tsx"),
  "utf8",
);

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(message);
}

assert(
  source.includes('disabled={audioPreparing || audioModStateLoading}'),
  "缺少账号时声纹开关仍应可点击并给出引导",
);
assert(
  source.includes('aria-label="声纹识别启用步骤"')
    && source.includes('label: "初始化账号"')
    && source.includes('label: "选择监听账号"')
    && source.includes('label: "准备识别 Mod"'),
  "自动化设置应展示完整的声纹启用步骤",
);
assert(
  source.includes("onInitializeAccount();") && source.includes('"初始化账号"'),
  "缺少账号时应提供可执行的初始化入口",
);
assert(
  source.includes('id="audio-prepare-blocked-reason"')
    && source.includes('还不能开始：')
    && source.includes('填写 Mod 名称后即可准备'),
  "准备按钮禁用时应在按钮附近说明缺少的配置",
);

console.log("audio onboarding integration tests passed");
