import { Component, createSignal, onMount, For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

// 类型定义
interface Summary {
  id: number;
  start_time: number;
  end_time: number;
  summary_type: string;
  content: string;
  structured_data: string | null;
  trace_count: number | null;
  created_at: number;
}

interface Entity {
  name: string;
  type: string;
}

interface ActivityCount {
  activity_type: string;
  count: number;
}

interface StructuredData {
  topics?: string[];
  links?: string[];
  activity_breakdown?: ActivityCount[];
  entities?: Entity[];
}

const Summaries: Component = () => {
  const [summaries, setSummaries] = createSignal<Summary[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [selectedSummary, setSelectedSummary] = createSignal<Summary | null>(null);
  const [summaryType, setSummaryType] = createSignal<string>("all");
  const [dateRange, setDateRange] = createSignal<number>(7); // 默认7天

  // 获取摘要列表
  const fetchSummaries = async () => {
    setLoading(true);
    try {
      const now = Date.now();
      const start = now - dateRange() * 24 * 60 * 60 * 1000;
      const type = summaryType() === "all" ? undefined : summaryType();

      const result = await invoke<Summary[]>("get_summaries", {
        startTime: start,
        endTime: now,
        summaryType: type,
        limit: 100,
      });
      setSummaries(result);
    } catch (e) {
      console.error("Failed to fetch summaries:", e);
    } finally {
      setLoading(false);
    }
  };

  // 删除摘要
  const deleteSummary = async (id: number) => {
    if (!confirm("确定要删除这条摘要吗？")) return;

    try {
      await invoke("delete_summary", { id });
      setSummaries(summaries().filter((s) => s.id !== id));
      if (selectedSummary()?.id === id) {
        setSelectedSummary(null);
      }
    } catch (e) {
      console.error("Failed to delete summary:", e);
    }
  };

  // 解析结构化数据
  const parseStructuredData = (data: string | null): StructuredData | null => {
    if (!data) return null;
    try {
      return JSON.parse(data);
    } catch {
      return null;
    }
  };

  // 格式化时间
  const formatTime = (timestamp: number) => {
    return new Date(timestamp).toLocaleString("zh-CN", {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  };

  // 格式化时间范围
  const formatTimeRange = (start: number, end: number) => {
    const startDate = new Date(start);
    const endDate = new Date(end);

    if (startDate.toDateString() === endDate.toDateString()) {
      return `${startDate.toLocaleDateString("zh-CN")} ${startDate.toLocaleTimeString(
        "zh-CN",
        { hour: "2-digit", minute: "2-digit" }
      )} - ${endDate.toLocaleTimeString("zh-CN", {
        hour: "2-digit",
        minute: "2-digit",
      })}`;
    }

    return `${formatTime(start)} - ${formatTime(end)}`;
  };

  // 获取摘要类型标签
  const getSummaryTypeLabel = (type: string) => {
    switch (type) {
      case "short":
        return "15分钟摘要";
      case "daily":
        return "每日摘要";
      default:
        return type;
    }
  };

  // 获取摘要类型颜色
  const getSummaryTypeColor = (type: string) => {
    switch (type) {
      case "short":
        return "bg-blue-500/20 text-blue-400";
      case "daily":
        return "bg-purple-500/20 text-purple-400";
      default:
        return "bg-gray-500/20 text-gray-400";
    }
  };

  // 获取实体类型颜色
  const getEntityTypeColor = (type: string) => {
    switch (type) {
      case "person":
        return "bg-green-500/20 text-green-400";
      case "project":
        return "bg-blue-500/20 text-blue-400";
      case "technology":
        return "bg-orange-500/20 text-orange-400";
      case "url":
        return "bg-cyan-500/20 text-cyan-400";
      case "file":
        return "bg-yellow-500/20 text-yellow-400";
      default:
        return "bg-gray-500/20 text-gray-400";
    }
  };

  onMount(() => {
    fetchSummaries();
  });

  return (
    <div class="h-full flex flex-col p-4 overflow-hidden">
      {/* 头部 */}
      <div class="flex items-center justify-between mb-4">
        <h2 class="text-xl font-bold">工作摘要</h2>

        <div class="flex items-center gap-4">
          {/* 类型过滤 */}
          <select
            value={summaryType()}
            onChange={(e) => {
              setSummaryType(e.target.value);
              fetchSummaries();
            }}
            class="px-3 py-2 bg-background-card border border-gray-700 rounded-lg text-sm"
          >
            <option value="all">全部类型</option>
            <option value="short">15分钟摘要</option>
            <option value="daily">每日摘要</option>
          </select>

          {/* 时间范围 */}
          <select
            value={dateRange()}
            onChange={(e) => {
              setDateRange(parseInt(e.target.value));
              fetchSummaries();
            }}
            class="px-3 py-2 bg-background-card border border-gray-700 rounded-lg text-sm"
          >
            <option value="1">今天</option>
            <option value="7">最近7天</option>
            <option value="30">最近30天</option>
            <option value="90">最近90天</option>
          </select>

          {/* 刷新按钮 */}
          <button
            onClick={fetchSummaries}
            disabled={loading()}
            class="px-3 py-2 bg-accent hover:bg-accent/80 disabled:opacity-50 rounded-lg text-sm transition-colors"
          >
            {loading() ? "加载中..." : "刷新"}
          </button>
        </div>
      </div>

      {/* 内容区 */}
      <div class="flex-1 flex gap-4 overflow-hidden">
        {/* 摘要列表 */}
        <div class="w-1/2 overflow-y-auto space-y-3 pr-2">
          <Show when={!loading()} fallback={<div class="text-center py-8 text-foreground-secondary">加载中...</div>}>
            <Show when={summaries().length > 0} fallback={
              <div class="text-center py-8 text-foreground-secondary">
                暂无摘要数据
              </div>
            }>
              <For each={summaries()}>
                {(summary) => (
                  <div
                    onClick={() => setSelectedSummary(summary)}
                    class={`p-4 bg-background-card rounded-lg cursor-pointer transition-all hover:ring-2 hover:ring-accent ${
                      selectedSummary()?.id === summary.id ? "ring-2 ring-accent" : ""
                    }`}
                  >
                    <div class="flex items-center justify-between mb-2">
                      <span
                        class={`px-2 py-1 rounded text-xs ${getSummaryTypeColor(
                          summary.summary_type
                        )}`}
                      >
                        {getSummaryTypeLabel(summary.summary_type)}
                      </span>
                      <span class="text-xs text-foreground-secondary">
                        {summary.trace_count || 0} 条记录
                      </span>
                    </div>

                    <p class="text-sm line-clamp-3 mb-2">{summary.content}</p>

                    <div class="flex items-center justify-between text-xs text-foreground-secondary">
                      <span>{formatTimeRange(summary.start_time, summary.end_time)}</span>
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          deleteSummary(summary.id);
                        }}
                        class="text-error hover:text-error/80"
                      >
                        删除
                      </button>
                    </div>
                  </div>
                )}
              </For>
            </Show>
          </Show>
        </div>

        {/* 摘要详情 */}
        <div class="w-1/2 overflow-y-auto bg-background-card rounded-lg p-4">
          <Show
            when={selectedSummary()}
            fallback={
              <div class="h-full flex items-center justify-center text-foreground-secondary">
                选择一条摘要查看详情
              </div>
            }
          >
            {(summary) => {
              const structured = parseStructuredData(summary().structured_data);

              return (
                <div class="space-y-4">
                  {/* 头部信息 */}
                  <div class="flex items-center justify-between">
                    <span
                      class={`px-2 py-1 rounded text-sm ${getSummaryTypeColor(
                        summary().summary_type
                      )}`}
                    >
                      {getSummaryTypeLabel(summary().summary_type)}
                    </span>
                    <span class="text-sm text-foreground-secondary">
                      {formatTimeRange(summary().start_time, summary().end_time)}
                    </span>
                  </div>

                  {/* 摘要内容 */}
                  <div>
                    <h3 class="text-sm font-semibold text-foreground-secondary mb-2">
                      摘要内容
                    </h3>
                    <p class="text-sm whitespace-pre-wrap">{summary().content}</p>
                  </div>

                  {/* 主题 */}
                  <Show when={structured?.topics && structured.topics.length > 0}>
                    <div>
                      <h3 class="text-sm font-semibold text-foreground-secondary mb-2">
                        主题
                      </h3>
                      <div class="flex flex-wrap gap-2">
                        <For each={structured?.topics}>
                          {(topic) => (
                            <span class="px-2 py-1 bg-accent/20 text-accent rounded text-sm">
                              {topic}
                            </span>
                          )}
                        </For>
                      </div>
                    </div>
                  </Show>

                  {/* 实体 */}
                  <Show when={structured?.entities && structured.entities.length > 0}>
                    <div>
                      <h3 class="text-sm font-semibold text-foreground-secondary mb-2">
                        提取的实体
                      </h3>
                      <div class="flex flex-wrap gap-2">
                        <For each={structured?.entities}>
                          {(entity) => (
                            <span
                              class={`px-2 py-1 rounded text-sm ${getEntityTypeColor(
                                entity.type
                              )}`}
                            >
                              {entity.name}
                              <span class="ml-1 opacity-60">({entity.type})</span>
                            </span>
                          )}
                        </For>
                      </div>
                    </div>
                  </Show>

                  {/* 活动分布 */}
                  <Show
                    when={
                      structured?.activity_breakdown &&
                      structured.activity_breakdown.length > 0
                    }
                  >
                    <div>
                      <h3 class="text-sm font-semibold text-foreground-secondary mb-2">
                        活动分布
                      </h3>
                      <div class="space-y-2">
                        <For each={structured?.activity_breakdown}>
                          {(activity) => (
                            <div class="flex items-center justify-between">
                              <span class="text-sm capitalize">
                                {activity.activity_type}
                              </span>
                              <span class="text-sm text-foreground-secondary">
                                {activity.count} 次
                              </span>
                            </div>
                          )}
                        </For>
                      </div>
                    </div>
                  </Show>

                  {/* 相关链接 */}
                  <Show when={structured?.links && structured.links.length > 0}>
                    <div>
                      <h3 class="text-sm font-semibold text-foreground-secondary mb-2">
                        相关链接/文件
                      </h3>
                      <div class="space-y-1">
                        <For each={structured?.links}>
                          {(link) => (
                            <p class="text-sm text-accent truncate">{link}</p>
                          )}
                        </For>
                      </div>
                    </div>
                  </Show>

                  {/* 元信息 */}
                  <div class="pt-4 border-t border-gray-700">
                    <div class="flex items-center justify-between text-xs text-foreground-secondary">
                      <span>记录数量: {summary().trace_count || 0}</span>
                      <span>创建时间: {formatTime(summary().created_at)}</span>
                    </div>
                  </div>
                </div>
              );
            }}
          </Show>
        </div>
      </div>
    </div>
  );
};

export default Summaries;
