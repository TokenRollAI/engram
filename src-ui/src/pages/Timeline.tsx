import { Component, createSignal, createEffect, For, Show, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { convertFileSrc } from "@tauri-apps/api/core";
import { format, startOfDay, endOfDay, addDays, subDays } from "date-fns";
import { zhCN } from "date-fns/locale";

// 类型定义
interface Trace {
  id: number;
  timestamp: number;
  image_path: string | null;
  app_name: string | null;
  window_title: string | null;
  is_fullscreen: boolean;
  is_idle: boolean;
  ocr_text: string | null;
  created_at: number;
}

const Timeline: Component = () => {
  const [selectedDate, setSelectedDate] = createSignal(new Date());
  const [traces, setTraces] = createSignal<Trace[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [selectedTrace, setSelectedTrace] = createSignal<Trace | null>(null);
  const [selectedImageSrc, setSelectedImageSrc] = createSignal<string | null>(null);
  const [collapsedHours, setCollapsedHours] = createSignal<Set<number>>(new Set());
  const [imageCache, setImageCache] = createSignal<Map<string, string>>(new Map());

  // 加载数据
  const loadTraces = async (date: Date) => {
    setLoading(true);
    try {
      const start = startOfDay(date).getTime();
      const end = endOfDay(date).getTime();

      const data = await invoke<Trace[]>("get_traces", {
        startTime: start,
        endTime: end,
        limit: 500,
        offset: 0,
      });

      setTraces(data);
      // 清空图片缓存
      setImageCache(new Map());
    } catch (e) {
      console.error("Failed to load traces:", e);
    } finally {
      setLoading(false);
    }
  };

  // 获取图片源
  const getImageSrc = async (relativePath: string): Promise<string | null> => {
    const cached = imageCache().get(relativePath);
    if (cached) return cached;

    try {
      const fullPath = await invoke<string>("get_image_path", { relativePath });
      const src = convertFileSrc(fullPath);
      setImageCache(prev => new Map(prev).set(relativePath, src));
      return src;
    } catch (e) {
      console.error("Failed to get image path:", e);
      return null;
    }
  };

  // 监听日期变化
  createEffect(() => {
    loadTraces(selectedDate());
  });

  // 日期导航
  const goToPreviousDay = () => setSelectedDate(subDays(selectedDate(), 1));
  const goToNextDay = () => setSelectedDate(addDays(selectedDate(), 1));
  const goToToday = () => setSelectedDate(new Date());

  // 按小时分组
  const tracesByHour = () => {
    const grouped: Record<number, Trace[]> = {};
    for (const trace of traces()) {
      const hour = new Date(trace.timestamp).getHours();
      if (!grouped[hour]) grouped[hour] = [];
      grouped[hour].push(trace);
    }
    return grouped;
  };

  // 切换小时折叠状态
  const toggleHourCollapse = (hour: number) => {
    setCollapsedHours(prev => {
      const newSet = new Set(prev);
      if (newSet.has(hour)) {
        newSet.delete(hour);
      } else {
        newSet.add(hour);
      }
      return newSet;
    });
  };

  // 全部展开/折叠
  const expandAll = () => setCollapsedHours(new Set<number>());
  const collapseAll = () => {
    const hours = Object.keys(tracesByHour()).map(Number);
    setCollapsedHours(new Set<number>(hours));
  };

  // 格式化时间
  const formatTime = (timestamp: number) => {
    return format(new Date(timestamp), "HH:mm:ss");
  };

  // 打开详情弹窗
  const openDetail = async (trace: Trace) => {
    setSelectedTrace(trace);
    if (trace.image_path) {
      const src = await getImageSrc(trace.image_path);
      setSelectedImageSrc(src);
    } else {
      setSelectedImageSrc(null);
    }
  };

  // 关闭详情弹窗
  const closeDetail = () => {
    setSelectedTrace(null);
    setSelectedImageSrc(null);
  };

  return (
    <div class="h-full flex flex-col">
      {/* 顶部工具栏 */}
      <header class="flex items-center justify-between px-6 py-4 border-b border-gray-700">
        <div class="flex items-center space-x-4">
          <button
            onClick={goToPreviousDay}
            class="p-2 hover:bg-background-card rounded transition-colors"
          >
            ◀
          </button>
          <h2 class="text-lg font-semibold">
            {format(selectedDate(), "yyyy年M月d日 EEEE", { locale: zhCN })}
          </h2>
          <button
            onClick={goToNextDay}
            class="p-2 hover:bg-background-card rounded transition-colors"
          >
            ▶
          </button>
          <button
            onClick={goToToday}
            class="px-3 py-1 text-sm bg-accent hover:bg-accent-hover rounded transition-colors"
          >
            今天
          </button>
        </div>

        <div class="flex items-center space-x-4">
          <div class="text-sm text-foreground-secondary">
            共 {traces().length} 条记录
          </div>
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

      {/* 主内容区 */}
      <div class="flex-1 overflow-auto p-6">
        <Show when={loading()}>
          <div class="flex items-center justify-center h-full">
            <p class="text-foreground-secondary">加载中...</p>
          </div>
        </Show>

        <Show when={!loading() && traces().length === 0}>
          <div class="flex flex-col items-center justify-center h-full text-foreground-secondary">
            <p class="text-4xl mb-4">📭</p>
            <p>当天没有记录</p>
          </div>
        </Show>

        <Show when={!loading() && traces().length > 0}>
          <div class="space-y-4">
            <For each={Object.entries(tracesByHour()).sort((a, b) => Number(a[0]) - Number(b[0]))}>
              {([hour, hourTraces]) => {
                const hourNum = Number(hour);
                const isCollapsed = () => collapsedHours().has(hourNum);

                return (
                  <div class="bg-background-card rounded-lg overflow-hidden">
                    {/* 小时标题 - 可点击折叠 */}
                    <div
                      class="flex items-center px-4 py-3 cursor-pointer hover:bg-gray-700/50 transition-colors"
                      onClick={() => toggleHourCollapse(hourNum)}
                    >
                      <span class="text-lg mr-2 transition-transform" style={{
                        transform: isCollapsed() ? "rotate(-90deg)" : "rotate(0deg)"
                      }}>
                        ▼
                      </span>
                      <span class="text-lg font-mono text-foreground-secondary">
                        {hour.toString().padStart(2, "0")}:00
                      </span>
                      <div class="flex-1 h-px bg-gray-700 mx-4" />
                      <span class="text-sm text-foreground-secondary">
                        {hourTraces.length} 条
                      </span>
                    </div>

                    {/* 截图网格 - 可折叠 */}
                    <Show when={!isCollapsed()}>
                      <div class="px-4 pb-4">
                        <div class="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 xl:grid-cols-8 gap-3">
                          <For each={hourTraces}>
                            {(trace) => (
                              <TraceCard
                                trace={trace}
                                getImageSrc={getImageSrc}
                                onClick={() => openDetail(trace)}
                                formatTime={formatTime}
                              />
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

      {/* 详情弹窗 */}
      <Show when={selectedTrace()}>
        <div
          class="fixed inset-0 bg-black/80 flex items-center justify-center z-50"
          onClick={closeDetail}
        >
          <div
            class="bg-background-secondary rounded-lg max-w-5xl max-h-[90vh] overflow-auto m-4"
            onClick={(e) => e.stopPropagation()}
          >
            {/* 图像预览 */}
            <div class="relative bg-background">
              <Show
                when={selectedImageSrc()}
                fallback={
                  <div class="aspect-video flex items-center justify-center text-foreground-secondary">
                    无图像
                  </div>
                }
              >
                <img
                  src={selectedImageSrc()!}
                  alt="Screenshot"
                  class="w-full h-auto max-h-[60vh] object-contain"
                />
              </Show>
            </div>

            {/* 详细信息 */}
            <div class="p-6">
              <h3 class="text-lg font-semibold mb-4">
                {selectedTrace()?.app_name || "未知应用"}
              </h3>

              <dl class="space-y-2 text-sm">
                <div class="flex">
                  <dt class="w-24 text-foreground-secondary">时间</dt>
                  <dd>
                    {selectedTrace() &&
                      format(
                        new Date(selectedTrace()!.timestamp),
                        "yyyy-MM-dd HH:mm:ss"
                      )}
                  </dd>
                </div>
                <div class="flex">
                  <dt class="w-24 text-foreground-secondary">窗口标题</dt>
                  <dd class="flex-1 break-all">{selectedTrace()?.window_title || "-"}</dd>
                </div>
                <div class="flex">
                  <dt class="w-24 text-foreground-secondary">全屏</dt>
                  <dd>{selectedTrace()?.is_fullscreen ? "是" : "否"}</dd>
                </div>
                <Show when={selectedTrace()?.ocr_text}>
                  <div>
                    <dt class="text-foreground-secondary mb-1">OCR 文本</dt>
                    <dd class="bg-background p-3 rounded text-xs font-mono max-h-40 overflow-auto whitespace-pre-wrap">
                      {selectedTrace()?.ocr_text}
                    </dd>
                  </div>
                </Show>
              </dl>

              <button
                class="mt-6 w-full py-2 bg-accent hover:bg-accent-hover rounded transition-colors"
                onClick={closeDetail}
              >
                关闭
              </button>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
};

// 独立的卡片组件，支持懒加载图片
const TraceCard: Component<{
  trace: Trace;
  getImageSrc: (path: string) => Promise<string | null>;
  onClick: () => void;
  formatTime: (ts: number) => string;
}> = (props) => {
  const [imageSrc, setImageSrc] = createSignal<string | null>(null);
  const [imageLoading, setImageLoading] = createSignal(true);
  const [imageError, setImageError] = createSignal(false);

  // 懒加载图片
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
      {/* 缩略图 */}
      <div class="aspect-video bg-background-secondary flex items-center justify-center overflow-hidden">
        <Show when={imageLoading()}>
          <span class="text-foreground-secondary text-xs animate-pulse">
            加载中...
          </span>
        </Show>
        <Show when={!imageLoading() && imageSrc() && !imageError()}>
          <img
            src={imageSrc()!}
            alt=""
            class="w-full h-full object-cover"
            onError={() => setImageError(true)}
          />
        </Show>
        <Show when={!imageLoading() && (!imageSrc() || imageError())}>
          <span class="text-foreground-secondary text-xs">
            无图像
          </span>
        </Show>
      </div>
      {/* 信息 */}
      <div class="p-2">
        <p class="text-xs font-medium truncate">
          {props.trace.app_name || "未知应用"}
        </p>
        <p class="text-xs text-foreground-secondary truncate">
          {props.trace.window_title || "-"}
        </p>
        <p class="text-xs text-foreground-secondary font-mono mt-1">
          {props.formatTime(props.trace.timestamp)}
        </p>
      </div>
    </div>
  );
};

export default Timeline;
