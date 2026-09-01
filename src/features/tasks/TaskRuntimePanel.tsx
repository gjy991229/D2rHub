import {
  Ban,
  CheckCircle2,
  ChevronDown,
  CircleX,
  Clock3,
  Loader2,
  RefreshCw,
  RotateCcw,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

import { Button } from "../../components/ui/Button";
import { taskGateway, type TaskGateway } from "./gateway";
import { subscribeBeforeReadingTasks, type TaskSnapshotMap } from "./taskSync";
import type { TaskSnapshot, TaskState, TaskTimelineEntry } from "./types";

type Language = "zh-CN" | "en-US";

interface TaskRuntimePanelProps {
  language?: string | null;
  gateway?: TaskGateway;
}

const COPY = {
  "zh-CN": {
    title: "后台任务",
    description: "启动、账号初始化、Mod 加工和自动跟房共用同一套状态、取消与诊断时间线。",
    loading: "正在读取任务状态",
    loadFailed: "任务状态暂时不可用",
    reload: "重新加载",
    emptyTitle: "还没有后台任务",
    emptyDescription: "开始启动账号、加工 Mod 或运行自动跟房后，任务会出现在这里。",
    active: "进行中",
    recent: "最近完成",
    cancel: "取消",
    cancelling: "正在取消",
    retry: "重试",
    timeline: "时间线",
    timelineFailed: "无法读取任务时间线",
    retryFailed: "无法重试此任务，请返回对应功能面板操作。",
    cancelFailed: "取消请求未能提交",
    noMessage: "等待下一步状态",
    states: {
      running: "进行中",
      succeeded: "已完成",
      failed: "失败",
      cancelled: "已取消",
    } satisfies Record<TaskState, string>,
  },
  "en-US": {
    title: "Background tasks",
    description: "Launches, account setup, Mod processing, and room automation share one status, cancellation, and diagnostic timeline.",
    loading: "Loading task status",
    loadFailed: "Task status is temporarily unavailable",
    reload: "Reload",
    emptyTitle: "No background tasks yet",
    emptyDescription: "Account launches, Mod processing, and room automation will appear here when they run.",
    active: "Active",
    recent: "Recent",
    cancel: "Cancel",
    cancelling: "Cancelling",
    retry: "Retry",
    timeline: "Timeline",
    timelineFailed: "Could not load the task timeline",
    retryFailed: "This task must be retried from its feature panel.",
    cancelFailed: "The cancellation request could not be submitted",
    noMessage: "Waiting for the next update",
    states: {
      running: "Running",
      succeeded: "Completed",
      failed: "Failed",
      cancelled: "Cancelled",
    } satisfies Record<TaskState, string>,
  },
} as const;

const KIND_LABELS: Record<Language, Record<string, string>> = {
  "zh-CN": {
    "account-launch": "账号启动",
    "battle-net-launch": "Battle.net 启动",
    "account-initialize": "账号初始化",
    "account-reinitialize": "账号重新初始化",
    "audio-mod-prepare": "Mod 加工",
    "audio-mod-upgrade": "Mod 更新",
    "room-automation": "自动跟房",
  },
  "en-US": {
    "account-launch": "Account launch",
    "battle-net-launch": "Battle.net launch",
    "account-initialize": "Account setup",
    "account-reinitialize": "Account reset",
    "audio-mod-prepare": "Mod processing",
    "audio-mod-upgrade": "Mod upgrade",
    "room-automation": "Room automation",
  },
};

function stateIcon(state: TaskState) {
  if (state === "succeeded") return CheckCircle2;
  if (state === "failed") return CircleX;
  if (state === "cancelled") return Ban;
  return Loader2;
}

function formatTime(timestamp: number, language: Language) {
  return new Intl.DateTimeFormat(language, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(timestamp);
}

export function TaskRuntimePanel({
  language,
  gateway = taskGateway,
}: TaskRuntimePanelProps) {
  const locale: Language = language === "en-US" ? "en-US" : "zh-CN";
  const copy = COPY[locale];
  const [tasks, setTasks] = useState<TaskSnapshotMap>(new Map());
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [operation, setOperation] = useState<{ taskId: number; kind: "cancel" | "retry" } | null>(null);
  const [operationError, setOperationError] = useState<string | null>(null);
  const [expandedTaskId, setExpandedTaskId] = useState<number | null>(null);
  const [timeline, setTimeline] = useState<Record<number, TaskTimelineEntry[]>>({});
  const [timelineError, setTimelineError] = useState<Record<number, string>>({});
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    setLoading(true);
    setLoadError(null);
    void subscribeBeforeReadingTasks(gateway, (snapshot) => {
      if (!disposed) {
        setTasks(snapshot);
        setLoading(false);
      }
    }).then((unsubscribe) => {
      if (disposed) unsubscribe();
      else stop = unsubscribe;
    }).catch((error) => {
      if (!disposed) {
        setLoading(false);
        setLoadError(error instanceof Error ? error.message : String(error));
      }
    });
    return () => {
      disposed = true;
      stop?.();
    };
  }, [gateway, reloadKey]);

  const orderedTasks = useMemo(() => Array.from(tasks.values()).sort((left, right) => {
    const leftActive = left.state === "running" ? 1 : 0;
    const rightActive = right.state === "running" ? 1 : 0;
    return rightActive - leftActive || right.started_at_ms - left.started_at_ms;
  }), [tasks]);

  const toggleTimeline = useCallback(async (task: TaskSnapshot) => {
    if (expandedTaskId === task.task_id) {
      setExpandedTaskId(null);
      return;
    }
    setExpandedTaskId(task.task_id);
    if (timeline[task.task_id]) return;
    try {
      const entries = await gateway.timeline(task.task_id);
      setTimeline(current => ({ ...current, [task.task_id]: entries }));
      setTimelineError(current => {
        const next = { ...current };
        delete next[task.task_id];
        return next;
      });
    } catch (error) {
      setTimelineError(current => ({
        ...current,
        [task.task_id]: error instanceof Error ? error.message : String(error),
      }));
    }
  }, [expandedTaskId, gateway, timeline]);

  const runOperation = async (task: TaskSnapshot, kind: "cancel" | "retry") => {
    setOperation({ taskId: task.task_id, kind });
    setOperationError(null);
    try {
      if (kind === "cancel") await gateway.cancel(task.task_id);
      else await gateway.retry(task.task_id);
    } catch {
      setOperationError(kind === "cancel" ? copy.cancelFailed : copy.retryFailed);
    } finally {
      setOperation(null);
    }
  };

  return (
    <section className="task-runtime-panel" aria-labelledby="task-runtime-title">
      <div className="task-runtime-heading">
        <div>
          <h2 id="task-runtime-title">{copy.title}</h2>
          <p>{copy.description}</p>
        </div>
        {!loading && (
          <Button size="sm" onClick={() => setReloadKey(key => key + 1)}>
            <RefreshCw size={12} aria-hidden="true" />
            {copy.reload}
          </Button>
        )}
      </div>

      {loading && (
        <div className="task-runtime-loading" role="status" aria-label={copy.loading}>
          <span /><span /><span />
        </div>
      )}

      {!loading && loadError && (
        <div className="task-runtime-empty" role="alert">
          <CircleX size={18} aria-hidden="true" />
          <div><strong>{copy.loadFailed}</strong><p>{loadError}</p></div>
        </div>
      )}

      {!loading && !loadError && orderedTasks.length === 0 && (
        <div className="task-runtime-empty">
          <Clock3 size={18} aria-hidden="true" />
          <div><strong>{copy.emptyTitle}</strong><p>{copy.emptyDescription}</p></div>
        </div>
      )}

      {operationError && <p className="task-runtime-operation-error" role="alert">{operationError}</p>}

      {!loading && !loadError && orderedTasks.length > 0 && (
        <div className="task-runtime-list" aria-live="polite">
          {orderedTasks.map((task, index) => {
            const Icon = stateIcon(task.state);
            const expanded = expandedTaskId === task.task_id;
            const activeOperation = operation?.taskId === task.task_id ? operation.kind : null;
            const previous = orderedTasks[index - 1];
            const showGroup = index === 0 || previous.state === "running" !== (task.state === "running");
            return (
              <div key={task.task_id} className="task-runtime-entry-wrap">
                {showGroup && (
                  <p className="task-runtime-group-label">
                    {task.state === "running" ? copy.active : copy.recent}
                  </p>
                )}
                <article className="task-runtime-entry" data-state={task.state}>
                  <Icon
                    size={16}
                    className={task.state === "running" ? "task-runtime-spin" : ""}
                    aria-hidden="true"
                  />
                  <div className="task-runtime-main">
                    <div className="task-runtime-title-line">
                      <strong>{KIND_LABELS[locale][task.kind] ?? task.kind}</strong>
                      <span data-state={task.state}>{copy.states[task.state]}</span>
                      <time dateTime={new Date(task.started_at_ms).toISOString()}>
                        {formatTime(task.started_at_ms, locale)}
                      </time>
                    </div>
                    <p>{task.message || copy.noMessage}</p>
                    {task.state === "running" && (
                      <div
                        className="task-runtime-progress"
                        role="progressbar"
                        aria-valuemin={0}
                        aria-valuemax={100}
                        aria-valuenow={task.progress}
                        aria-label={`${KIND_LABELS[locale][task.kind] ?? task.kind} ${task.progress}%`}
                      >
                        <span style={{ width: `${task.progress}%` }} />
                      </div>
                    )}
                  </div>
                  <div className="task-runtime-actions">
                    {task.state === "running" && (
                      <Button
                        size="sm"
                        variant="danger"
                        disabled={task.cancel_requested || activeOperation !== null}
                        onClick={() => void runOperation(task, "cancel")}
                      >
                        <X size={12} aria-hidden="true" />
                        {task.cancel_requested ? copy.cancelling : copy.cancel}
                      </Button>
                    )}
                    {task.state !== "running" && task.retryable && task.state !== "succeeded" && (
                      <Button
                        size="sm"
                        disabled={activeOperation !== null}
                        onClick={() => void runOperation(task, "retry")}
                      >
                        <RotateCcw size={12} aria-hidden="true" />
                        {copy.retry}
                      </Button>
                    )}
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-expanded={expanded}
                      onClick={() => void toggleTimeline(task)}
                    >
                      <ChevronDown size={13} className={expanded ? "task-runtime-chevron-open" : ""} aria-hidden="true" />
                      {copy.timeline}
                    </Button>
                  </div>
                  {expanded && (
                    <div className="task-runtime-timeline">
                      {timelineError[task.task_id] && <p role="alert">{copy.timelineFailed}</p>}
                      {!timelineError[task.task_id] && !timeline[task.task_id] && (
                        <Loader2 size={14} className="task-runtime-spin" aria-label={copy.loading} />
                      )}
                      {timeline[task.task_id]?.map(entry => (
                        <div key={entry.revision} className="task-runtime-timeline-row">
                          <time>{formatTime(entry.timestamp_ms, locale)}</time>
                          <span>{entry.step}</span>
                          <p>{entry.message || copy.noMessage}</p>
                        </div>
                      ))}
                    </div>
                  )}
                </article>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}
