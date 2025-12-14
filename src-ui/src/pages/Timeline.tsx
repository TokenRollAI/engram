import { Component, createSignal, createEffect, For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
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
    } catch (e) {
      console.error("Failed to load traces:", e);
    } finally {
      setLoading(false);
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

  // 格式化时间
  const formatTime = (timestamp: number) => {
    return format(new Date(timestamp), "HH:mm:ss");
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

        <div class="text-sm text-foreground-secondary">
          共 {traces().length} 条记录
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
          <div class="space-y-8">
            <For each={Object.entries(tracesByHour()).sort((a, b) => Number(a[0]) - Number(b[0]))}>
              {([hour, hourTraces]) => (
                <div>
                  {/* 小时标题 */}
                  <div class="flex items-center mb-4">
                    <span class="text-lg font-mono text-foreground-secondary">
                      {hour.padStart(2, "0")}:00
                    </span>
                    <div class="flex-1 h-px bg-gray-700 ml-4" />
                    <span class="text-sm text-foreground-secondary ml-4">
                      {hourTraces.length} 条
                    </span>
                  </div>

                  {/* 截图网格 */}
                  <div class="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 xl:grid-cols-8 gap-3">
                    <For each={hourTraces}>
                      {(trace) => (
                        <div
                          class="bg-background-card rounded-lg overflow-hidden cursor-pointer hover:ring-2 hover:ring-accent transition-all"
                          onClick={() => setSelectedTrace(trace)}
                        >
                          {/* 缩略图占位符 */}
                          <div class="aspect-video bg-background-secondary flex items-center justify-center">
                            <Show
                              when={trace.image_path}
                              fallback={
                                <span class="text-foreground-secondary text-xs">
                                  无图像
                                </span>
                              }
                            >
                              <span class="text-foreground-secondary text-2xl">
                                🖼️
                              </span>
                            </Show>
                          </div>
                          {/* 信息 */}
                          <div class="p-2">
                            <p class="text-xs font-medium truncate">
                              {trace.app_name || "未知应用"}
                            </p>
                            <p class="text-xs text-foreground-secondary truncate">
                              {trace.window_title || "-"}
                            </p>
                            <p class="text-xs text-foreground-secondary font-mono mt-1">
                              {formatTime(trace.timestamp)}
                            </p>
                          </div>
                        </div>
                      )}
                    </For>
                  </div>
                </div>
              )}
            </For>
          </div>
        </Show>
      </div>

      {/* 详情弹窗 */}
      <Show when={selectedTrace()}>
        <div
          class="fixed inset-0 bg-black/80 flex items-center justify-center z-50"
          onClick={() => setSelectedTrace(null)}
        >
          <div
            class="bg-background-secondary rounded-lg max-w-4xl max-h-[90vh] overflow-auto m-4"
            onClick={(e) => e.stopPropagation()}
          >
            {/* 图像预览 */}
            <div class="aspect-video bg-background flex items-center justify-center">
              <span class="text-foreground-secondary">图像预览</span>
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
                  <dd>{selectedTrace()?.window_title || "-"}</dd>
                </div>
                <div class="flex">
                  <dt class="w-24 text-foreground-secondary">全屏</dt>
                  <dd>{selectedTrace()?.is_fullscreen ? "是" : "否"}</dd>
                </div>
                <Show when={selectedTrace()?.ocr_text}>
                  <div>
                    <dt class="text-foreground-secondary mb-1">OCR 文本</dt>
                    <dd class="bg-background p-3 rounded text-xs font-mono max-h-40 overflow-auto">
                      {selectedTrace()?.ocr_text}
                    </dd>
                  </div>
                </Show>
              </dl>

              <button
                class="mt-6 w-full py-2 bg-accent hover:bg-accent-hover rounded transition-colors"
                onClick={() => setSelectedTrace(null)}
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

export default Timeline;
