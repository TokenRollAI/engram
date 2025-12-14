import { Component, createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { addDays, endOfDay, format, startOfDay, subDays } from "date-fns";
import { zhCN } from "date-fns/locale";

interface ActivitySession {
  id: number;
  app_name: string;
  start_time: number;
  end_time: number;
  trace_count: number;
  context_text: string | null;
  key_actions_json?: string | null;
}

interface Trace {
  id: number;
  timestamp: number;
  image_path: string | null;
  app_name: string | null;
  window_title: string | null;
  is_fullscreen: boolean;
  is_idle: boolean;
  ocr_text: string | null;
  activity_session_id?: number | null;
  is_key_action?: boolean;
  created_at: number;
}

interface ImageData {
  mime: string;
  bytes: number[];
}

const Timeline: Component = () => {
  const [selectedDate, setSelectedDate] = createSignal(new Date());
  const [sessions, setSessions] = createSignal<ActivitySession[]>([]);
  const [loading, setLoading] = createSignal(false);

  const [selectedSession, setSelectedSession] = createSignal<ActivitySession | null>(null);
  const [sessionTraces, setSessionTraces] = createSignal<Trace[]>([]);
  const [selectedTrace, setSelectedTrace] = createSignal<Trace | null>(null);
  const [selectedImageSrc, setSelectedImageSrc] = createSignal<string | null>(null);

  const [collapsedHours, setCollapsedHours] = createSignal<Set<number>>(new Set());
  const [imageCache, setImageCache] = createSignal<Map<string, string>>(new Map());

  const revokeImageCache = (cache: Map<string, string>) => {
    for (const url of cache.values()) {
      URL.revokeObjectURL(url);
    }
  };

  const getImageSrc = async (relativePath: string): Promise<string | null> => {
    const cached = imageCache().get(relativePath);
    if (cached) return cached;

    try {
      const payload = await invoke<ImageData>("get_image_data", { relativePath });
      const blob = new Blob([new Uint8Array(payload.bytes)], { type: payload.mime || "image/jpeg" });
      const src = URL.createObjectURL(blob);
      setImageCache(prev => new Map(prev).set(relativePath, src));
      return src;
    } catch (e) {
      console.error("Failed to get image data:", e);
      return null;
    }
  };

  const loadSessions = async (date: Date) => {
    setLoading(true);
    try {
      const start = startOfDay(date).getTime();
      const end = endOfDay(date).getTime();

      const data = await invoke<ActivitySession[]>("get_activity_sessions", {
        startTime: start,
        endTime: end,
        limit: 200,
        offset: 0,
        appFilter: null,
      });
      setSessions(data);

      setImageCache(prev => {
        revokeImageCache(prev);
        return new Map();
      });
    } catch (e) {
      console.error("Failed to load sessions:", e);
      setSessions([]);
    } finally {
      setLoading(false);
    }
  };

  createEffect(() => {
    loadSessions(selectedDate());
  });

  onCleanup(() => {
    revokeImageCache(imageCache());
  });

  const goToPreviousDay = () => setSelectedDate(subDays(selectedDate(), 1));
  const goToNextDay = () => setSelectedDate(addDays(selectedDate(), 1));
  const goToToday = () => setSelectedDate(new Date());

  const sessionsByHour = () => {
    const grouped: Record<number, ActivitySession[]> = {};
    for (const session of sessions()) {
      const hour = new Date(session.end_time).getHours();
      if (!grouped[hour]) grouped[hour] = [];
      grouped[hour].push(session);
    }
    return grouped;
  };

  const toggleHourCollapse = (hour: number) => {
    setCollapsedHours(prev => {
      const next = new Set(prev);
      if (next.has(hour)) next.delete(hour);
      else next.add(hour);
      return next;
    });
  };

  const expandAll = () => setCollapsedHours(new Set<number>());
  const collapseAll = () => {
    const hours = Object.keys(sessionsByHour()).map(Number);
    setCollapsedHours(new Set<number>(hours));
  };

  const openSessionDetail = async (session: ActivitySession) => {
    setSelectedSession(session);
    setSelectedTrace(null);
    setSelectedImageSrc(null);
    try {
      const traces = await invoke<Trace[]>("get_activity_session_traces", {
        sessionId: session.id,
        limit: 500,
        offset: 0,
      });
      setSessionTraces(traces);
    } catch (e) {
      console.error("Failed to load session traces:", e);
      setSessionTraces([]);
    }
  };

  const closeDetail = () => {
    setSelectedSession(null);
    setSessionTraces([]);
    setSelectedTrace(null);
    setSelectedImageSrc(null);
  };

  const formatTime = (timestamp: number) => format(new Date(timestamp), "HH:mm:ss");

  return (
    <div class="h-full flex flex-col">
      <header class="flex items-center justify-between px-6 py-4 border-b border-gray-700">
        <div class="flex items-center space-x-4">
          <button onClick={goToPreviousDay} class="p-2 hover:bg-background-card rounded transition-colors">
            ◀
          </button>
          <h2 class="text-lg font-semibold">
            {format(selectedDate(), "yyyy年M月d日 EEEE", { locale: zhCN })}
          </h2>
          <button onClick={goToNextDay} class="p-2 hover:bg-background-card rounded transition-colors">
            ▶
          </button>
          <button onClick={goToToday} class="px-3 py-1 text-sm bg-accent hover:bg-accent-hover rounded transition-colors">
            今天
          </button>
        </div>

        <div class="flex items-center space-x-4">
          <div class="text-sm text-foreground-secondary">共 {sessions().length} 个 Session</div>
          <div class="flex space-x-2">
            <button
              onClick={expandAll}
              class="px-2 py-1 text-xs bg-background-card hover:bg-gray-700 rounded transition-colors"
              title="展开全部"
            >
              展开
            </button>
            <button
              onClick={collapseAll}
              class="px-2 py-1 text-xs bg-background-card hover:bg-gray-700 rounded transition-colors"
              title="折叠全部"
            >
              折叠
            </button>
          </div>
        </div>
      </header>

      <div class="flex-1 overflow-auto p-6">
        <Show when={loading()}>
          <div class="flex items-center justify-center h-full">
            <p class="text-foreground-secondary">加载中...</p>
          </div>
        </Show>

        <Show when={!loading() && sessions().length === 0}>
          <div class="flex flex-col items-center justify-center h-full text-foreground-secondary">
            <p class="text-4xl mb-4">📭</p>
            <p>当天没有记录</p>
          </div>
        </Show>

        <Show when={!loading() && sessions().length > 0}>
          <div class="space-y-4">
            <For each={Object.entries(sessionsByHour()).sort((a, b) => Number(a[0]) - Number(b[0]))}>
              {([hour, hourSessions]) => {
                const hourNum = Number(hour);
                const isCollapsed = () => collapsedHours().has(hourNum);

                return (
                  <div class="bg-background-card rounded-lg overflow-hidden">
                    <div
                      class="flex items-center px-4 py-3 cursor-pointer hover:bg-gray-700/50 transition-colors"
                      onClick={() => toggleHourCollapse(hourNum)}
                    >
                      <span
                        class="text-lg mr-2 transition-transform"
                        style={{ transform: isCollapsed() ? "rotate(-90deg)" : "rotate(0deg)" }}
                      >
                        ▼
                      </span>
                      <span class="text-lg font-mono text-foreground-secondary">
                        {hour.toString().padStart(2, "0")}:00
                      </span>
                      <div class="flex-1 h-px bg-gray-700 mx-4" />
                      <span class="text-sm text-foreground-secondary">{hourSessions.length} 个</span>
                    </div>

                    <Show when={!isCollapsed()}>
                      <div class="px-4 pb-4">
                        <div class="space-y-3">
                          <For each={hourSessions}>
                            {(session) => (
                              <button
                                class="w-full text-left bg-background rounded-lg p-3 hover:ring-2 hover:ring-accent transition-all"
                                onClick={() => openSessionDetail(session)}
                              >
                                <div class="flex items-center justify-between gap-4">
                                  <div class="min-w-0">
                                    <div class="font-medium truncate">{session.app_name}</div>
                                    <div class="text-xs text-foreground-secondary font-mono mt-1">
                                      {format(new Date(session.start_time), "HH:mm")} -{" "}
                                      {format(new Date(session.end_time), "HH:mm")}
                                      {"  "}·{"  "}
                                      {session.trace_count} 条 Trace
                                    </div>
                                  </div>
                                  <div class="text-xs text-foreground-secondary">查看</div>
                                </div>
                                <Show when={session.context_text}>
                                  <div class="mt-2 text-xs text-foreground-secondary whitespace-pre-wrap max-h-20 overflow-hidden">
                                    {session.context_text}
                                  </div>
                                </Show>
                              </button>
                            )}
                          </For>
                        </div>
                      </div>
                    </Show>
                  </div>
                );
              }}
            </For>
          </div>
        </Show>
      </div>

      <Show when={selectedSession()}>
        <div class="fixed inset-0 bg-black/80 flex items-center justify-center z-50" onClick={closeDetail}>
          <div
            class="bg-background-secondary rounded-lg max-w-5xl max-h-[90vh] overflow-auto m-4"
            onClick={(e) => e.stopPropagation()}
          >
            <div class="p-6">
              <h3 class="text-lg font-semibold mb-4">{selectedSession()!.app_name}</h3>

              <dl class="space-y-2 text-sm">
                <div class="flex">
                  <dt class="w-24 text-foreground-secondary">时间</dt>
                  <dd>
                    {format(new Date(selectedSession()!.start_time), "yyyy-MM-dd HH:mm")} -{" "}
                    {format(new Date(selectedSession()!.end_time), "HH:mm")}
                  </dd>
                </div>
                <div class="flex">
                  <dt class="w-24 text-foreground-secondary">Trace 数</dt>
                  <dd>{selectedSession()!.trace_count}</dd>
                </div>
                <Show when={selectedSession()?.context_text}>
                  <div>
                    <dt class="text-foreground-secondary mb-1">Session 结论</dt>
                    <dd class="bg-background p-3 rounded text-xs font-mono max-h-40 overflow-auto whitespace-pre-wrap">
                      {selectedSession()?.context_text}
                    </dd>
                  </div>
                </Show>
              </dl>

              <Show when={selectedSession()?.key_actions_json}>
                <div class="mt-6">
                  <h4 class="text-sm font-semibold mb-3">关键行为</h4>
                  <KeyActionsPanel keyActionsJson={selectedSession()!.key_actions_json!} />
                </div>
              </Show>

              <div class="mt-6">
                <h4 class="text-sm font-semibold mb-3">本 Session 的 Traces</h4>
                <Show when={sessionTraces().length > 0} fallback={<div class="text-sm text-foreground-secondary">暂无 traces</div>}>
                  <div class="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 gap-3">
                    <For each={sessionTraces()}>
                      {(trace) => (
                        <TraceCard
                          trace={trace}
                          getImageSrc={getImageSrc}
                          onClick={async () => {
                            setSelectedTrace(trace);
                            if (trace.image_path) {
                              const src = await getImageSrc(trace.image_path);
                              setSelectedImageSrc(src);
                            } else {
                              setSelectedImageSrc(null);
                            }
                          }}
                          formatTime={formatTime}
                        />
                      )}
                    </For>
                  </div>
                </Show>
              </div>

              <Show when={selectedTrace()}>
                <div class="mt-6 border-t border-gray-700 pt-4">
                  <h4 class="text-sm font-semibold mb-3">Trace 详情</h4>
                  <Show when={selectedImageSrc()}>
                    <img
                      src={selectedImageSrc()!}
                      alt="Screenshot"
                      class="w-full h-auto max-h-[50vh] object-contain bg-background rounded"
                    />
                  </Show>
                  <dl class="mt-3 space-y-2 text-sm">
                    <div class="flex">
                      <dt class="w-24 text-foreground-secondary">时间</dt>
                      <dd>{format(new Date(selectedTrace()!.timestamp), "yyyy-MM-dd HH:mm:ss")}</dd>
                    </div>
                    <div class="flex">
                      <dt class="w-24 text-foreground-secondary">窗口标题</dt>
                      <dd class="flex-1 break-all">{selectedTrace()!.window_title || "-"}</dd>
                    </div>
                    <Show when={selectedTrace()!.ocr_text}>
                      <div>
                        <dt class="text-foreground-secondary mb-1">OCR 文本</dt>
                        <dd class="bg-background p-3 rounded text-xs font-mono max-h-40 overflow-auto whitespace-pre-wrap">
                          {selectedTrace()!.ocr_text}
                        </dd>
                      </div>
                    </Show>
                  </dl>
                </div>
              </Show>

              <button class="mt-6 w-full py-2 bg-accent hover:bg-accent-hover rounded transition-colors" onClick={closeDetail}>
                关闭
              </button>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
};

const TraceCard: Component<{
  trace: Trace;
  getImageSrc: (path: string) => Promise<string | null>;
  onClick: () => void;
  formatTime: (ts: number) => string;
}> = (props) => {
  const [imageSrc, setImageSrc] = createSignal<string | null>(null);
  const [imageLoading, setImageLoading] = createSignal(true);
  const [imageError, setImageError] = createSignal(false);

  onMount(async () => {
    if (props.trace.image_path) {
      try {
        const src = await props.getImageSrc(props.trace.image_path);
        setImageSrc(src);
      } catch {
        setImageError(true);
      }
    }
    setImageLoading(false);
  });

  return (
    <div
      class="bg-background rounded-lg overflow-hidden cursor-pointer hover:ring-2 hover:ring-accent transition-all"
      onClick={props.onClick}
    >
      <div class="aspect-video bg-background-secondary flex items-center justify-center overflow-hidden">
        <Show when={imageLoading()}>
          <span class="text-foreground-secondary text-xs animate-pulse">加载中...</span>
        </Show>
        <Show when={!imageLoading() && imageSrc() && !imageError()}>
          <img src={imageSrc()!} alt="" class="w-full h-full object-cover" onError={() => setImageError(true)} />
        </Show>
        <Show when={!imageLoading() && (!imageSrc() || imageError())}>
          <span class="text-foreground-secondary text-xs">无图像</span>
        </Show>
      </div>
      <div class="p-2">
        <div class="flex items-center justify-between gap-2">
          <p class="text-xs font-medium truncate">{props.trace.app_name || "未知应用"}</p>
          <Show when={props.trace.is_key_action}>
            <span class="text-[10px] px-1.5 py-0.5 rounded bg-accent/30 text-white">关键</span>
          </Show>
        </div>
        <p class="text-xs text-foreground-secondary truncate">{props.trace.window_title || "-"}</p>
        <p class="text-xs text-foreground-secondary font-mono mt-1">{props.formatTime(props.trace.timestamp)}</p>
      </div>
    </div>
  );
};

const KeyActionsPanel: Component<{ keyActionsJson: string }> = (props) => {
  const items = () => {
    try {
      const arr = JSON.parse(props.keyActionsJson);
      return Array.isArray(arr) ? arr : [];
    } catch {
      return [];
    }
  };

  return (
    <div class="bg-background rounded p-3 text-xs space-y-2 max-h-56 overflow-auto">
      <Show when={items().length > 0} fallback={<div class="text-foreground-secondary">暂无关键行为</div>}>
        <For each={items().slice().reverse()}>
          {(it: any) => (
            <div class="border-b border-gray-700/60 pb-2 last:border-b-0 last:pb-0">
              <div class="text-foreground-secondary font-mono">
                {it.timestamp ? format(new Date(it.timestamp), "HH:mm:ss") : "--:--:--"} · Trace #{it.trace_id ?? "?"}
              </div>
              <div class="mt-1 whitespace-pre-wrap">
                {it.action_description || it.summary || "-"}
              </div>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
};

export default Timeline;
