import { useState, useEffect } from "react";
import { Heart, HeartHandshake, Copy, Check, ZoomIn, X, ChevronRight, ChevronDown, Github } from "lucide-react";
import { Modal } from "../components/ui/Modal";
import UpdateConfirmModal from "../components/ui/UpdateConfirmModal";
import { invoke } from "@tauri-apps/api/core";
import { showToast } from "../components/ui/Toast";

interface Props {
  open: boolean;
  onClose: () => void;
}

export function AboutModal({ open, onClose }: Props) {
  const [copied, setCopied] = useState(false);
  const [previewImage, setPreviewImage] = useState<string | null>(null);
  const [showSponsors, setShowSponsors] = useState(false);
  const [showDonation, setShowDonation] = useState(false);
  const [version, setVersion] = useState("0.1.0");
  const [checking, setChecking] = useState(false);
  const [updateStatus, setUpdateStatus] = useState<string | null>(null);
  const [lastCheckTime, setLastCheckTime] = useState(0);

  // Custom modal update states
  const [showUpdateConfirm, setShowUpdateConfirm] = useState(false);
  const [pendingUpdateUrl, setPendingUpdateUrl] = useState("");
  const [pendingVersion, setPendingVersion] = useState("");


  const qq = "1070676143";
  const sponsors = ["rabbitxman#1168", "忙着搞数学"];

  useEffect(() => {
    if (!open) return;
    (async () => {
      try {
        const ver = await invoke<string>("get_app_version");
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
        download_url: string;
      }
      const info = await invoke<CloudVersionInfo>("check_cloud_version");
      const cloudVersion = info.version;
      const downloadUrl = info.download_url;

      const cleanLocal = version.replace(/^v/, "").trim();
      const cleanCloud = cloudVersion.replace(/^v/, "").trim();

      if (cleanLocal === cleanCloud) {
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
              <p className="text-xs text-text-muted/60">
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
                className="text-xs text-text-muted/60 hover:text-text-primary flex items-center gap-0.5 font-medium cursor-pointer"
                title="打开 GitHub 仓库"
              >
                <Github size={11} />
                GitHub
              </button>
            </div>
          </div>

          {/* QQ */}
          <div className="rounded-xl px-3.5 py-2.5 flex items-center justify-between"
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

          {/* Sponsors */}
          <div className="pt-1">
            <button
              type="button"
              aria-expanded={showSponsors}
              aria-controls="about-sponsor-list"
              onClick={() => setShowSponsors(!showSponsors)}
              className="w-full flex items-center justify-between py-1 hover:text-text-primary transition-colors cursor-pointer text-left"
            >
              <div className="flex items-center gap-1.5">
                <HeartHandshake size={12} className="text-accent" />
                <span className="text-md font-medium text-text-primary">感谢赞助</span>
              </div>
              <span className="text-text-muted text-xs flex items-center gap-0.5 font-medium">
                {showSponsors ? "折叠" : "展开"}
                {showSponsors ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
              </span>
            </button>

            <div
              id="about-sponsor-list"
              aria-hidden={!showSponsors}
              className="overflow-hidden transition-all duration-200 ease-in-out motion-reduce:transition-none"
              style={{
                maxHeight: showSponsors ? "120px" : "0px",
                opacity: showSponsors ? 1 : 0,
              }}
            >
              <div
                className="mt-3 rounded-xl px-3.5"
                style={{ background: "var(--surface-hover)" }}
              >
                {sponsors.map((sponsor, index) => (
                  <div
                    key={sponsor}
                    className="flex items-center gap-2.5 py-2.5"
                    style={index > 0 ? { borderTop: "1px solid var(--border-default)" } : undefined}
                  >
                    <span className="w-1.5 h-1.5 rounded-full bg-accent shrink-0" aria-hidden="true" />
                    <span className="text-sm font-medium text-text-secondary">{sponsor}</span>
                  </div>
                ))}
              </div>
            </div>
          </div>

          {/* Donation */}
          <div className="pt-1">
            <button
              type="button"
              aria-expanded={showDonation}
              aria-controls="about-donation-codes"
              onClick={() => setShowDonation(!showDonation)}
              className="w-full flex items-center justify-between py-1 hover:text-text-primary transition-colors cursor-pointer text-left"
            >
              <div className="flex items-center gap-1.5">
                <Heart size={12} className="text-accent" />
                <span className="text-md font-medium text-text-primary">赞助支持作者</span>
              </div>
              <span className="text-text-muted text-xs flex items-center gap-0.5 font-medium">
                {showDonation ? "折叠" : "展开"}
                {showDonation ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
              </span>
            </button>

            <div
              id="about-donation-codes"
              aria-hidden={!showDonation}
              className="overflow-hidden transition-all duration-200 ease-in-out motion-reduce:transition-none"
              style={{
                maxHeight: showDonation ? "300px" : "0px",
                opacity: showDonation ? 1 : 0,
              }}
            >
              <div className="grid grid-cols-2 gap-3 mt-3">
                {[
                  { src: "/vx.jpg", label: "微信" },
                  { src: "/zfb.jpg", label: "支付宝" },
                ].map(qr => (
                  <div key={qr.src} className="text-center group">
                    <div className="aspect-[3/4] bg-white rounded-xl overflow-hidden relative
                      hover:ring-1 hover:ring-accent/30 transition-all"
                      style={{ border: "1px solid var(--border-default)" }}>
                      <button
                        onClick={() => setPreviewImage(qr.src)}
                        tabIndex={showDonation ? 0 : -1}
                        className="w-full h-full block cursor-pointer border-0 p-0 bg-transparent"
                        aria-label={`放大 ${qr.label} 收款码`}>
                        <img src={qr.src} alt={qr.label}
                          className="w-full h-full object-cover"
                          onError={e => {
                            (e.target as HTMLImageElement).style.display = "none";
                            const p = (e.target as HTMLImageElement).parentElement?.parentElement;
                            if (p) {
                              const s = document.createElement("span");
                              s.className = "text-xs text-text-muted absolute inset-0 flex items-center justify-center";
                              s.textContent = `缺少 ${qr.src}`;
                              p.appendChild(s);
                            }
                          }}
                        />
                      </button>
                      <div className="absolute inset-0 bg-black/0 group-hover:bg-black/30
                        flex items-center justify-center transition-colors pointer-events-none">
                        <ZoomIn size={22} className="text-white opacity-0 group-hover:opacity-100 transition-opacity" />
                      </div>
                    </div>
                    <p className="text-xs text-text-muted mt-1.5">{qr.label}</p>
                  </div>
                ))}
              </div>
            </div>
          </div>

          <p className="text-2xs text-text-muted text-center">
            Built with Tauri + React
          </p>
        </div>
      </Modal>

      {/* Full-screen preview */}
      {previewImage && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center"
          style={{ background: "rgba(0,0,0,0.85)" }}
          onClick={() => setPreviewImage(null)}>
          <button onClick={() => setPreviewImage(null)}
            className="absolute top-4 right-4 p-2 rounded-lg text-white/70 hover:text-white
              hover:bg-white/10 transition-colors z-10">
            <X size={22} />
          </button>
          <div className="bg-white rounded-2xl p-6 max-w-[360px] max-h-[80vh]"
            style={{ boxShadow: "var(--shadow-elevated)" }}
            onClick={e => e.stopPropagation()}>
            <img src={previewImage} alt="收款码"
              className="w-full h-full object-contain max-h-[65vh] rounded-lg" />
            <p className="text-center text-gray-500 text-xs mt-3">请使用对应 App 扫描</p>
          </div>
        </div>
      )}

      <UpdateConfirmModal
        open={showUpdateConfirm}
        onClose={() => setShowUpdateConfirm(false)}
        version={pendingVersion}
        downloadUrl={pendingUpdateUrl}
      />
    </>
  );
}
