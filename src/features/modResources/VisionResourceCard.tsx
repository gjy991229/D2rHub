import { useEffect, useRef, useState } from "react";
import { Download, FolderOpen, RefreshCw } from "lucide-react";
import { Button } from "../../components/ui/Button";
import { ProgressBar } from "../../components/ui/ProgressBar";
import { visionResourcesGateway, type VisionResourcePackage } from "./visionResourcesGateway";

export function VisionResourceCard({ en }: { en: boolean }) {
  const [data, setData] = useState<VisionResourcePackage | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [readError, setReadError] = useState("");
  const live = useRef(false);
  const operation = useRef(false);
  const revision = useRef(0);

  useEffect(() => {
    live.current = true;
    const read = async (refresh = false) => {
      const current = ++revision.current;
      try {
        const value = await visionResourcesGateway.read(refresh);
        if (live.current && current === revision.current) { setData(value); setReadError(""); }
      } catch (e) {
        if (live.current && current === revision.current) setReadError(String(e));
      }
    };
    void read(true);
    const timer = setInterval(() => void read(), 1500);
    return () => { live.current = false; revision.current += 1; clearInterval(timer); };
  }, []);

  const prepare = async () => {
    if (operation.current) return;
    operation.current = true;
    setPending(true);
    setError("");
    setData(value => value && { ...value, preparing: true, progress: null, message: en ? "Preparing resources…" : "正在准备视觉识别资源包…" });
    revision.current += 1;
    try {
      const result = await visionResourcesGateway.prepare();
      if (live.current) { revision.current += 1; setData(result); }
    } catch (e) {
      if (live.current) {
        revision.current += 1;
        setError(String(e));
        setData(value => value && { ...value, preparing: false, ready: false, progress: null, message: String(e) });
      }
    } finally {
      operation.current = false;
      if (live.current) setPending(false);
    }
  };
  const active = pending || !!data?.preparing;
  const title = en ? "Visual recognition resources" : "视觉识别资源包";

  return <article className="resource-card" aria-label={title} aria-busy={active}>
    <div className="resource-summary">
      <div className="resource-title"><strong>{title}</strong>
        {data && <span>{en ? "Download" : "下载"} {(data.size / 1048576).toFixed(1)} MB</span>}
      </div>
      <p>{en ? "Shared by scene recognition and room signals. Download once for all accounts and game editions."
        : "用于地区识别和聊天跟房；所有账号与游戏版本共用，下载一次即可。"}</p>
    </div>
    <div className="resource-metadata">
      <small>{en ? "Gitee mirror available; follows your download source setting." : "支持 Gitee 国内镜像，遵循软件的下载源设置。"}</small>
      <p className="resource-install-state" data-state={data?.ready ? "ready" : active ? "update" : "neutral"} role="status">{data?.message || (en ? "Checking local resources…" : "正在检查本地资源…")}</p>
      <small>{en ? "Recognition still prepares missing resources automatically when needed." : "未提前下载时，启用识别仍会自动准备所需资源。"}</small>
    </div>
    <div className="resource-actions">
      <Button size="sm" variant="primary" disabled={active} onClick={() => void prepare()}>
        {data?.ready ? <RefreshCw size={13} /> : <Download size={13} />}
        {active ? (en ? "Preparing…" : "正在准备…") : data?.ready ? (en ? "Verify resources" : "校验资源包") : (en ? "Download & prepare" : "下载并准备")}
      </Button>
      <Button size="sm" variant="ghost" onClick={() => {
        void visionResourcesGateway.openFolder().catch(e => { if (live.current) setError(String(e)); });
      }}><FolderOpen size={12} />{en ? "Open folder" : "打开目录"}</Button>
    </div>
    {active && <div className="resource-progress" role="status" aria-live="polite">
      <ProgressBar label={en ? "Visual resources progress" : "视觉识别资源包进度"} value={data?.preparing ? data.progress ?? undefined : undefined} />
      <small>{en ? "You can return here to check preparation progress." : "可离开页面，返回后继续查看准备进度。"}</small>
    </div>}
    {(error || readError) && <p className="resource-error" role="alert">{error || readError}</p>}
  </article>;
}
