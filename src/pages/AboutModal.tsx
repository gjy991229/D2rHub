import { useState, useEffect, type CSSProperties } from "react";
import { Heart, HeartHandshake, Copy, Check, Github } from "lucide-react";
import { Modal } from "../components/ui/Modal";
import UpdateConfirmModal from "../components/ui/UpdateConfirmModal";
import { invokeCommand } from "../platform/tauri";
import { showToast } from "../components/ui/Toast";
import { useI18n } from "../i18n";
import "./about.css";
import { prefersReducedMotion } from "../store/motionPreference";

interface Props {
  open: boolean;
  onClose: () => void;
}

export function AboutModal({ open, onClose }: Props) {
  const { language } = useI18n();
  const [copied, setCopied] = useState(false);
  const [celebrating, setCelebrating] = useState(false);
  const [sponsors, setSponsors] = useState<string[]>([]);
  const [sponsorsLoaded, setSponsorsLoaded] = useState(false);
  const [page, setPage] = useState<"thanks" | "support" | null>(null);
  const en = language === "en-US";
  const thanksTitle = en ? "Sponsor acknowledgements" : "感谢赞助名单";
  const supportTitle = en ? "Support the author" : "赞助支持作者";
  const [version, setVersion] = useState("0.1.0");
  const [checking, setChecking] = useState(false);
  const [updateStatus, setUpdateStatus] = useState<string | null>(null);
  const [lastCheckTime, setLastCheckTime] = useState(0);

  // Custom modal update states
  const [showUpdateConfirm, setShowUpdateConfirm] = useState(false);
  const [pendingUpdateUrl, setPendingUpdateUrl] = useState("");
  const [pendingVersion, setPendingVersion] = useState("");


  const qq = "1070676143";

  useEffect(() => {
    let cancelled = false;

    fetch("/sponsors.json")
      .then(response => {
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        return response.json() as Promise<unknown>;
      })
      .then(data => {
        if (!Array.isArray(data)) throw new Error("赞助名单必须是 JSON 数组");

        const names = Array.from(new Set(
          data
            .filter((name): name is string => typeof name === "string")
            .map(name => name.trim())
            .filter(Boolean),
        ));

        if (!cancelled) setSponsors(names);
      })
      .catch(error => {
        console.error("无法加载赞助名单:", error);
        if (!cancelled) setSponsors([]);
      })
      .finally(() => {
        if (!cancelled) setSponsorsLoaded(true);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!open) {
      setPage(null);
      setCelebrating(false);
      return;
    }
    (async () => {
      try {
        const ver = await invokeCommand<string>("get_app_version");
        setVersion(ver);

        // Check if there is an update flag saved
        const storedCloudVer = localStorage.getItem("d2rhub-update-available-version");
        const cleanLocal = ver.replace(/^v/, "").trim();
        if (storedCloudVer && storedCloudVer !== cleanLocal) {
          setUpdateStatus("有可用更新");
        } else {
          setUpdateStatus(null);
        }
      } catch {}
    })();
  }, [open]);

  useEffect(() => {
    if (!celebrating) return;
    const timer = window.setTimeout(() => setCelebrating(false), 1350);
    return () => window.clearTimeout(timer);
  }, [celebrating]);

  const closeSupport = () => {
    setPage(null);
    setCelebrating(!prefersReducedMotion());
  };

  const copyQQ = async () => {
    try {
      await navigator.clipboard.writeText(qq);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      const i = document.createElement("input");
      i.value = qq;
      document.body.appendChild(i);
      i.select();
      document.execCommand("copy");
      document.body.removeChild(i);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  const openGithub = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-shell");
      await open("https://github.com/gjy991229/D2RHub");
    } catch (err) {
      console.error("无法打开 GitHub 链接:", err);
      showToast("error", "打开外部浏览器失败，请手动访问 GitHub 仓库");
    }
  };

  const handleCheckUpdate = async () => {
    const now = Date.now();
    if (now - lastCheckTime < 10000) {
      showToast("warning", "请勿频繁检查更新");
      return;
    }
    setLastCheckTime(now);
    setChecking(true);
    setUpdateStatus("正在检查...");

    try {
      interface CloudVersionInfo {
        version: string;
        available: boolean;
      }
      const info = await invokeCommand<CloudVersionInfo>("check_software_update");
      const cloudVersion = info.version;
      const downloadUrl = "";

      const cleanCloud = cloudVersion.replace(/^v/, "").trim();

      if (!info.available) {
        setUpdateStatus("已是最新版本");
        showToast("success", "当前已是最新版本");
        localStorage.removeItem("d2rhub-update-available-version");
      } else {
        setUpdateStatus("发现新版本");
        localStorage.setItem("d2rhub-update-available-version", cleanCloud);

        // Open custom UI-styled confirmation modal instead of tauri's native popups
        setPendingVersion(cloudVersion);
        setPendingUpdateUrl(downloadUrl);
        setShowUpdateConfirm(true);
      }
    } catch (e) {
      setUpdateStatus("检查失败");
      showToast("error", `检查更新失败: ${e}`);
    } finally {
      setChecking(false);
    }
  };

  return (
    <>
      <Modal open={open} onClose={onClose} title="关于" width="max-w-sm">
        <div className="space-y-5">
          {/* App info */}
          <div className="text-center">
            <img src="/logo.png" alt="D2RHub"
              className="w-12 h-12 rounded-xl object-contain mx-auto mb-3"
            />
            <h3 className="text-sm font-semibold text-text-primary">D2RHub</h3>
            <p className="text-sm text-text-muted mt-0.5">
              Diablo II Resurrected 多账号管理
            </p>
            <div className="flex items-center justify-center gap-2 mt-1">
              <p className="text-xs text-text-muted">
                版本 v{version}
              </p>
              <span className="text-xs text-text-muted/30">•</span>
              <button
                onClick={handleCheckUpdate}
                disabled={checking}
                className="text-xs text-accent hover:underline font-medium cursor-pointer"
              >
                {checking ? "正在检查..." : (updateStatus || "检查更新")}
              </button>
              <span className="text-xs text-text-muted/30">•</span>
              <button
                onClick={openGithub}
                className="text-xs text-text-muted hover:text-text-primary flex items-center gap-0.5 font-medium cursor-pointer"
                title="打开 GitHub 仓库"
              >
                <Github size={11} />
                GitHub
              </button>
            </div>
          </div>

          {/* QQ */}
          <div className="rounded-xl px-3.5 py-2.5 flex flex-wrap items-center justify-between gap-2"
            style={{ background: "var(--surface-hover)" }}>
            <span className="text-md text-text-secondary">交流反馈QQ群 {qq}</span>
            <button onClick={copyQQ}
              className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-sm
                text-text-secondary hover:text-text-primary transition-colors"
              style={{ border: "1px solid var(--border-default)" }}>
              {copied
                ? <><Check size={12} className="text-success" />已复制</>
                : <><Copy size={12} />复制</>
              }
            </button>
          </div>

          <div className="about-links">
            {celebrating && <div className="about-fireworks" aria-hidden="true">
              {[0, 1, 2].map(burst => <div className="about-burst" key={burst}>
                {Array.from({ length: 12 }, (_, i) => <i key={i} style={{
                  "--angle": `${i * 30 + burst * 12}deg`,
                  "--reach": i % 2 ? "-30px" : "-44px",
                } as CSSProperties} />)}
              </div>)}
            </div>}
            <button type="button" aria-haspopup="dialog" onClick={() => setPage("thanks")}>
              <HeartHandshake size={16} aria-hidden="true" /><span>{thanksTitle}</span>
              {sponsors.length > 0 && <small aria-hidden="true">{sponsors.length}</small>}
            </button>
            <button type="button" aria-haspopup="dialog" onClick={() => { setCelebrating(false); setPage("support"); }}>
              <Heart size={16} aria-hidden="true" /><span>{supportTitle}</span>
            </button>
          </div>

          <p className="text-2xs text-text-muted text-center">
            Built with Tauri + React
          </p>
        </div>
      </Modal>

      <Modal open={open && page === "thanks"} onClose={() => setPage(null)} title={thanksTitle} width="max-w-md">
        {!sponsorsLoaded ? (
          <p role="status" className="text-sm text-text-muted">{en ? "Loading sponsors…" : "正在加载名单…"}</p>
        ) : sponsors.length ? (
          <ul className="about-sponsors" aria-label={en ? "Sponsors" : "赞助者名单"} data-i18n-skip>
            {sponsors.map(name => <li key={name}>{name}</li>)}
          </ul>
        ) : (
          <p className="text-sm text-text-muted">{en ? "No sponsors listed" : "暂无赞助名单"}</p>
        )}
      </Modal>

      <Modal open={open && page === "support"} onClose={closeSupport} title={supportTitle} width="max-w-3xl">
        <div className="about-payment-options">
          <img src="/vx.jpg" alt={en ? "WeChat Pay QR code" : "微信收款码"} />
          <img src="/zfb.jpg" alt={en ? "Alipay QR code" : "支付宝收款码"} />
        </div>
      </Modal>

      <UpdateConfirmModal
        open={showUpdateConfirm}
        onClose={() => setShowUpdateConfirm(false)}
        version={pendingVersion}
        downloadUrl={pendingUpdateUrl}
      />
    </>
  );
}
