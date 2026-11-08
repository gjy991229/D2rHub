import { useRef } from "react";
import { Modal } from "./ui/Modal";
import { Button } from "./ui/Button";
import { useLaunchModConfirmation } from "../store/launchModConfirmation";

export function LaunchModConfirmation() {
  const { warning, respond } = useLaunchModConfirmation();
  const returnButton = useRef<HTMLButtonElement>(null);
  return <Modal open={warning !== null} onClose={() => respond(false)}
    title="启动前检查" width="max-w-lg" initialFocusRef={returnButton}
    footer={<div className="flex flex-wrap justify-end gap-2">
      <Button ref={returnButton} variant="ghost" onClick={() => respond(false)}>返回调整</Button>
      <Button variant="primary" onClick={() => respond(true)}>跳过并启动</Button>
    </div>}>
    <div className="space-y-4 py-1 text-sm leading-relaxed text-text-secondary">
      <p className="font-semibold text-text-primary">本次启动的 Mod 缺少所需功能</p>
      <div className="space-y-3">{warning?.split("\n").filter(Boolean).map((problem, index) =>
        <p key={index} className="rounded-lg border border-border bg-surface-card p-3 break-words">{problem}</p>
      )}</div>
      <p className="text-xs text-text-muted">继续启动将跳过不支持的识别或跟房功能，账号和方案的 Mod 配置保持不变。</p>
    </div>
  </Modal>;
}

