import { useSyncExternalStore, type ReactNode } from "react";
import { CheckCircle, AlertCircle, XCircle, Info, X } from "lucide-react";
import { createPortal } from "react-dom";
import { useI18n } from "../../i18n";

type ToastType = "success" | "warning" | "error" | "info";

interface ToastItem {
  id: number;
  type: ToastType;
  message: string;
}

let nextId = 0;
const listeners = new Set<() => void>();
let toasts: ToastItem[] = [];

function setToasts(next: ToastItem[]) {
  toasts = next;
  listeners.forEach(listener => listener());
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}

function getSnapshot() { return toasts; }

function dismissToast(id: number) {
  if (toasts.some(toast => toast.id === id)) setToasts(toasts.filter(toast => toast.id !== id));
}

export function showToast(type: ToastType, message: string) {
  if (type === "error" && toasts.some(toast => toast.type === type && toast.message === message)) return;
  const toast: ToastItem = { id: nextId++, type, message };
  setToasts([...toasts, toast]);
  if (type !== "error") setTimeout(() => dismissToast(toast.id), 4000);
}

const iconMap: Record<ToastType, ReactNode> = {
  success: <CheckCircle size={16} />,
  warning: <AlertCircle size={16} />,
  error: <XCircle size={16} />,
  info: <Info size={16} />,
};

export function ToastContainer() {
  const { language } = useI18n();
  const toasts = useSyncExternalStore(subscribe, getSnapshot);

  if (toasts.length === 0) return null;

  return createPortal(
    <div className="fixed bottom-4 right-4 z-[100] flex flex-col-reverse gap-2 max-w-xs" style={{ maxWidth: "min(320px, calc(100vw - 32px))", maxHeight: "calc(100vh - 32px)", overflowY: "auto" }}>
      {toasts.map((toast) => (
        <div
          key={toast.id}
          role={toast.type === "error" ? "alert" : "status"}
          className={`toast-enter toast-item toast-${toast.type}`}
        >
          <span className="shrink-0 toast-icon">{iconMap[toast.type]}</span>
          <p className="toast-message">
            {toast.message}
          </p>
          <button
            type="button"
            aria-label={language === "en-US" ? "Dismiss notification" : "关闭提示"}
            onClick={() => dismissToast(toast.id)}
            className="toast-close-btn"
          >
            <X size={14} />
          </button>
        </div>
      ))}
    </div>, document.body
  );
}
