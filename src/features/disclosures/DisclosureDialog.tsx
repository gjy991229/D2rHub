import { useEffect, useState } from "react";
import { AlertTriangle, Check, ShieldCheck } from "lucide-react";
import { Button } from "../../components/ui/Button";
import { Modal } from "../../components/ui/Modal";
import type { OptionalModuleTabId, SettingsLanguage } from "../settings/settingsRegistry";
import {
  applicationDisclosureCopy,
  moduleDisclosureCopy,
} from "./disclosureCopy";
import "./disclosures.css";

const ACCEPT_DELAY_SECONDS = 3;

type DisclosureTarget =
  | { type: "application"; version: string | null }
  | { type: "module"; module: OptionalModuleTabId };

interface DisclosureDialogProps {
  open: boolean;
  language: SettingsLanguage;
  target: DisclosureTarget;
  onAccept: () => void | Promise<void>;
  onCancel: () => void;
  accepting?: boolean;
  canceling?: boolean;
}

export function DisclosureDialog({
  open,
  language,
  target,
  onAccept,
  onCancel,
  accepting = false,
  canceling = false,
}: DisclosureDialogProps) {
  const [remainingSeconds, setRemainingSeconds] = useState(ACCEPT_DELAY_SECONDS);
  const applicationLevel = target.type === "application";
  const copy = applicationLevel
    ? applicationDisclosureCopy(language, target.version)
    : moduleDisclosureCopy(language, target.module);
  const countdownId = applicationLevel
    ? "application-disclosure-countdown"
    : `module-${target.module}-disclosure-countdown`;

  useEffect(() => {
    if (!open) return;
    const deadline = Date.now() + ACCEPT_DELAY_SECONDS * 1000;
    setRemainingSeconds(ACCEPT_DELAY_SECONDS);
    const timer = window.setInterval(() => {
      const next = Math.max(0, Math.ceil((deadline - Date.now()) / 1000));
      setRemainingSeconds(next);
      if (next === 0) window.clearInterval(timer);
    }, 100);
    return () => window.clearInterval(timer);
  }, [open, target.type, target.type === "module" ? target.module : target.version]);

  const waiting = remainingSeconds > 0;
  const acceptLabel = waiting
    ? language === "en-US"
      ? `${copy.accept} (${remainingSeconds}s)`
      : `${copy.accept}（${remainingSeconds}s）`
    : copy.accept;

  return (
    <Modal
      open={open}
      onClose={onCancel}
      title={copy.title}
      width={applicationLevel ? "max-w-3xl" : "max-w-2xl"}
      dismissible={!applicationLevel && !accepting && !canceling}
      footer={
        <div className="disclosure-actions">
          <Button variant="ghost" disabled={accepting} loading={canceling} onClick={onCancel}>
            {copy.cancel}
          </Button>
          <Button
            variant="primary"
            disabled={waiting || accepting || canceling}
            loading={accepting}
            aria-describedby={countdownId}
            onClick={() => void onAccept()}
          >
            {!accepting && <Check size={13} aria-hidden="true" />}
            {acceptLabel}
          </Button>
        </div>
      }
    >
      <div className="disclosure-dialog" data-level={applicationLevel ? "application" : "module"}>
        <div className="disclosure-intro">
          <span className="disclosure-intro-icon" aria-hidden="true">
            <ShieldCheck size={18} strokeWidth={1.8} />
          </span>
          <div>
            <span className="disclosure-context">{copy.context}</span>
            <p>{copy.summary}</p>
          </div>
        </div>

        <div
          className="disclosure-scroll"
          tabIndex={0}
          aria-label={language === "en-US" ? "Disclosure details" : "说明正文"}
        >
          {copy.sections.map((section) => (
            <section
              className="disclosure-section"
              data-tone={section.tone}
              key={section.title}
            >
              <h3>
                {section.tone === "warning" && <AlertTriangle size={14} aria-hidden="true" />}
                {section.title}
              </h3>
              <p>{section.body}</p>
            </section>
          ))}
        </div>

        <p id={countdownId} className="disclosure-countdown" aria-live="polite">
          {waiting
            ? `${copy.waiting}${language === "en-US" ? ": " : "："}${remainingSeconds}s`
            : copy.accept}
        </p>
      </div>
    </Modal>
  );
}
